# P102 — The top bar belongs to the OS; we only add our icons

**User directive, 2026-08-19:** «il semaforo non lo devi creare te ma deve dipendere dal SO. In
generale la top bar deve essere quella del sistema operativo + le nostre icone.»

Confirmed the same day, when asked to choose: «Non deve disegnare 3 cerchi propri.» And on how:
**«Sì, risolvi come Zed.»**

We must not draw window controls. The window's decorations come from the operating system, and
our own icons sit alongside them. **Zed's handling is the instruction, not merely a reference** —
it is a GPUI application that already runs on Linux under both decoration regimes, and the standing
goal names Zed's crates as the place to read GPUI patterns from. Read it at
`~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346` (its own UI crates, not gpui) and follow what
it does; do not invent a scheme alongside it.

One thing that must survive the translation, because the two instructions only look contradictory:
"do not draw our own controls" and "do it like Zed" agree once you notice that Zed does not draw
unconditionally either — it asks the platform. The rule is **never draw when the OS is drawing**,
not "never draw". See the risk section below for why the unconditional reading would be a worse
defect than the one being fixed.

**Corrected 2026-08-19 by the scout, and it sharpens the reconciliation rather than breaking it.**
Zed's *default* on Linux is **client-side** decorations, not server-side:
`assets/settings/default.json` sets `"window_decorations": "client"` and `WindowDecorations::Client`
is `#[default]` in `settings_content/src/workspace.rs`. Zed draws its own titlebar and its own
controls by default, and only stops when a user opts into `window_decorations: "server"`. That is
the *opposite* posture from this port, which requests `Server`.

So "resolve it like Zed" cannot mean "copy Zed's default", because Zed's default is the very thing
the user rejected. What transfers is Zed's **mechanism**, not its policy: it reads
`window.window_decorations()` per render at ~6 independent call sites and matches on it, so every
element that touches the window edge decides for itself. We keep our `Server` request — which is
what makes the OS draw the bar the user asked for — and adopt Zed's conditional so that under
`Client` (a compositor that refuses SSD) the window is still closable. Policy ours, mechanism Zed's.

Note also that Zed reads the setting **once**, at window creation (`zed.rs::build_window_options`),
and never calls `request_decorations` again; live CSD↔SSD transitions only reach the app through
Wayland's `xdg-decoration` `Configure`, announced over the generic `on_appearance_changed` callback
that also carries OS light/dark changes. X11 has no renegotiation channel at all — it decides once
and never upgrades a resolved `Server` decision if a WM appears later.

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

## Measured, 2026-08-19: we do ask the window manager to decorate us

Run live from a private Xvfb (booted with `-displayfd`, killed afterwards), against
`/dev/shm/tt/debug/tiller`. The app's mapped window carries:

```
_MOTIF_WM_HINTS(_MOTIF_WM_HINTS) = 0x2, 0x0, 0x1, 0x0, 0x0
```

That is `[1 << 1, 0, 1, 0, 0]` verbatim — the `WindowDecorations::Server` arm of
`x11/window.rs:1874`. Flags = `MWM_HINTS_DECORATIONS`, decorations = **1, enabled**.

Two notes on reading this correctly. The app creates six X windows and only one is mapped; a probe
that takes the last id finds an unmapped one where the property is genuinely absent, which is how
the first attempt at this measurement produced a false negative. Match on `IsViewable` first. And
the vendored `rust/vendor/gpui_linux` is byte-identical to upstream on this path — it is patched
only for the `wl_data_device` `Drop` race (P133) — so there is no local override to suspect.

**What this proves and what it does not.** It proves our side of the contract: we ask for
server-side decorations, so `titlebar.rs`'s premise that "GPUI on X11 hands us a bare window" does
not hold for the revision we pin. It does not prove what a window manager does with the hint,
because no window manager is installed on this machine (`openbox`, `marco`, `xfwm4`, `mutter`,
`i3`, `fluxbox`, `metacity`, `twm`, `icewm` are all absent) and the user's own `cosmic-comp` is off
limits. Under Xvfb nothing decorates anything, so the visual half of the question is still open.

Which means the likely case is now the strongly-favoured one: a real WM honours the hint and draws
a titlebar, and we draw three more controls beneath it.

## The remaining measurement, and then the change

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

## Landed 2026-08-19 — `85270f5e`, merged into `linux/gpui-waku`

`tiller_ui/src/titlebar.rs` reads `Window::window_decorations()` fresh every render and builds the
three-dot cluster only under `Decorations::Client { .. }`. Under `Server` nothing is constructed
where they were — no div, no hitbox, not "drawn but hidden". The icon cluster is unconditional in
both branches, and its leading edge moves from `traffic_light_cluster_gap` to `traffic_light_inset`
when the lights are absent, so it does not stay pinned to a gap that follows a group nobody drew.

macOS is carved out with `cfg!(target_os = "macos")` rather than left to fall out of the
`Decorations` match. This is load-bearing and not defensive coding: AppKit's `PlatformWindow` never
overrides `window_decorations()`, so it inherits gpui's blanket `Server` default — without the
carve-out this same logic would silently stop drawing on macOS too. Zed does the same thing for the
same reason, hard-coding `PlatformStyle::Mac => None` instead of letting a generic match decide.

A test seam was needed and is worth knowing about: `TestWindow` always answers `Server`, so a drawn
test can never reach the `Client` branch on its own. `TitleBar::with_decorations` injects an
override, which is how both branches were verified rather than only the one the harness can reach.

**Critic verdict:** compiled, launched, both branches verified live, macOS path untouched, window
still closable, 425 tests passing (`tiller_ui` 368 + `tiller_theme` 57), no new clippy warnings.

### The one thing still unverified, and it needs software this box does not have

**Nobody has seen this rendered under a real window manager.** None is installed (`openbox`,
`marco`, `xfwm4`, `mutter`, `i3`, `fluxbox`, `metacity`, `twm`, `icewm` all absent) and the user's
`cosmic-comp` is off limits. What is proven is the code half: under `Server` our row draws zero
dots, confirmed by a real screenshot, and the `_MOTIF_WM_HINTS` bit pattern is unchanged and live
(`0x2, 0x0, 0x1, 0x0, 0x0` — decorations enabled). What is *not* proven is the visual outcome:
whether the OS titlebar and our icon row read as one coherent bar or as an orphaned icon strip
floating under someone else's chrome. That is a design question the user asked to be shown rather
than designed around, and it needs either a lightweight WM installed here or the user's own eyes on
COSMIC.

### A dead token the critic refused to let pass quietly

`Spacing::traffic_light_inset` (`tiller_theme/src/lib.rs:500`, default `px(14.0)` at `:535`) now has
**zero consumers** in the Rust tree except its own `Default` init and a bare value assertion at
`:2047`. It is pre-existing, not introduced by `85270f5e` — but it is exactly the
dead-token-with-a-live-test shape this task's brief warned about, which is how a deleted feature
comes back. Note the lookalike trap before touching it: this is a *different* field from the
`BrowserChrome::traffic_light_inset` the new code reads, and macOS still needs the latter.

# Hidden-Pane Rendering — Investigation Notes (Spike Steps 2–3)

**Date:** 2026-07-17
**Status:** candidate API found — decision pending Step 1 (GPU profile) and Step 4 (AC7 sentinel gate)

## Live Memory Measurement (v0.5.0, ~9 panes)

| Metric | Value |
|--------|-------|
| Physical footprint | 775 MB (peak 900 MB) |
| IOSurface (dirty) | 497 MB — 64% of footprint |
| IOAccelerator + graphics unmapped | ~134 MB |
| MALLOC heap | 89 MB |
| IOSurface regions owned by Tiller | ~19 × ~17 MB |

RAM is dominated by GPU surfaces, not heap. The ring-buffer work (Phase 3) can
recover at most a few MB; the hidden-pane rendering path is the dominant lever
for AC5.

## Candidate API (Step 2)

`GhosttyTerminal.AppTerminalView.setSurfaceVisible(_ visible: Bool)` — `open`
public API (`AppTerminalView.swift:38-40`), forwards to
`TerminalSurfaceCoordinator.setDisplayVisible(_:)`
(`TerminalSurfaceCoordinator.swift:230-244`), which:

1. Calls `ghostty_surface_set_occlusion(surface, visible)` — the same occlusion
   mechanism upstream Ghostty.app uses when a window is occluded.
2. Stops the per-surface display link when not visible
   (`canRenderFrame == false → stopDisplayLink()`), and restarts rendering with
   `requestImmediateTick()` on re-show.

The framework never calls this itself: `viewDidMoveToWindow` only handles
attach/detach, and no `NSWindow.didChangeOcclusionStateNotification` observer
exists in the package. It is explicitly a host-app API.

**Tiller never calls it.** Hidden worktrees are hidden via
`.opacity(isVisible ? 1 : 0)` (`App/ContentView.swift:290`), which libghostty
cannot see — every hidden pane keeps its display link ticking at refresh rate
and keeps its Metal drawables alive.

## Step 3 Evaluation

| Criterion | Result |
|-----------|--------|
| Documented public API | Yes — `open func setSurfaceVisible` |
| Tested in libghostty test suite | No tests found; production use: `ghostty_surface_set_occlusion` is Ghostty.app's standard occluded-window path |
| No data loss on resume | Expected (occlusion pauses rendering only; PTY/scrollback independent) — **must be proven by AC7 sentinel gate** |
| Thread-safe | Main-actor UI call, same as all view API |
| macOS 15+ | Yes |

Spike constraints respected: this is not a surface disconnect, not an
`alphaValue = 0` workaround, and not an undocumented internal API.

## Remaining Gates (manual)

- **Step 1:** Instruments GPU profile — hidden panes must exceed 5% of GPU
  time per frame for the change to be worth the risk. (Given every hidden pane
  runs its own display link, this is very likely, but must be measured.)
- **Step 4:** AC7 sentinel sequence — hide pane, feed `seq 1 1000` with
  markers, re-show, assert byte-exact scrollback.

If both pass, the implementation is small: propagate `isVisible` from
`ContentView`/`SplitViewRenderer` down to `setSurfaceVisible(_:)` on each
pane's terminal view.

## Note on RAM vs CPU expectations

Stopping the display link is a certain CPU/GPU win. Whether occlusion also
releases Metal drawables (IOSurface memory) is not guaranteed by the API and
must be read from the footprint delta during Step 1.

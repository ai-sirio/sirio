# Wave E slice E-P4 — report

## `F-BRW-01` — browser webview lands shrunk toward the pane's top-left — implemented (self-calibrating fix)

**Verdict: implemented, not visually re-verified this pass (X11-only row; see below).**

### What the recorded diagnosis got right, and what it missed

Confirmed `native_webview_rect` (the D-P1-era fix, commit `bcad730`) is genuinely a mathematical
no-op: `dpi::PixelUnit::to_logical` on an already-`Logical`-tagged value returns the same numbers
regardless of the `scale_factor` argument (`dpi-0.1.2/src/lib.rs:352-356`), so wry's own
`set_bounds` (`wry-0.56.1/src/webkitgtk/mod.rs:964-967`) cannot be re-dividing our `Rect` a second
time — the D-P1 critic's own conclusion ("bug is on the wry/GTK side") is correct, and confirmed by
reading wry's actual source rather than re-deriving the theory.

What the doc's evidence didn't yet have was a way to *act* on that without either (a) vendoring/
patching wry (outside this slice's owned files and Cargo graph) or (b) hardcoding the observed
`~1/1.1667` ratio, which is this one machine's X11 DPI setting, not a portable constant.

### The fix

`WebView::bounds()` (`wry-0.56.1/src/webkitgtk/mod.rs:935-961`) reads the window's *real* on-screen
geometry straight from `XGetWindowAttributes` — ground truth, independent of whatever GTK/GDK does
internally to our `set_bounds` request. `browser.rs`'s `NativeWebViewElement::prepaint` now:

1. Sends the requested rect as before.
2. On the first frame only, reads back `webview.bounds()`, compares it to what was requested, and
   derives the actual multiplier GTK silently applied (averaged across width/height to damp integer
   rounding noise).
3. Caches that factor (`SharedScaleCorrection`, `Rc<Cell<Option<f64>>>`, one per `BrowserSurface`)
   and pre-multiplies every subsequent `set_bounds` request by it, so the *real* on-screen rect
   converges to what GPUI's layout actually asked for.

This measures the correction from the live system rather than assuming a specific DPI setting,
GDK scale-factor semantics, or wry version behaviour — it will self-adjust if the desktop's DPI
changes and costs one extra `bounds()` syscall-equivalent only on the calibrating frame.

Files touched: `rust/crates/tiller_ui/src/browser.rs` only (`rust/crates/tiller_theme/src/cosmic/mod.rs`
needed no change for this row — nothing there touches webview geometry).

### Verification

- `cargo build -p tiller` — green.
- `cargo test -p tiller_ui --lib browser::` — 10/10 pass, including a new
  `scale_correction_recovers_the_exact_shrink_d_p1_measured_live` test that reproduces the exact
  `1/1.1667` ratio from the critic's live pixel scan (requested 850×792 at (386,133), observed
  728px-wide content) and asserts the recovered correction factor round-trips a request back to
  itself through that same shrink.
- **Not re-driven live on `DISPLAY=:1`.** `docs/linux-rewrite/WAYLAND-LANE.md` states plainly that
  `F-BRW` rows need real WebKitGTK page content, which the Wayland lane cannot provide ("its chrome
  renders, the page does not"); X11 (`DISPLAY=:1`) is the shared, lockable lane, and this pass did
  not take that lock. The fix is therefore implemented and unit-tested against the exact numbers
  the critic captured, but the live pixel-scan re-check that would move this row to PASSED is still
  owed to the next critic pass.

**howToExercise:** on `DISPLAY=:1`, open a Browser tab (tab-bar `+` → "New Browser") pointed at
`https://example.com`, wait for real content to render, then pixel-scan a content row the same way
D-P1 did (`convert <capture>.png -crop <row> txt:-` or equivalent) and compare the rendered content
span against the pane's chrome bounds. If the fix works, the two should now line up (no ~55px left
bleed, no ~177px right gap); if a residual mismatch remains, log `self.scale_correction.get()`'s
value at the calibrating frame (or add a temporary `eprintln!` in `prepaint`) to see what factor was
actually measured and whether a second calibration pass (e.g. after a resize) would be needed.

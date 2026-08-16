# Wave G slice G1-browser — report

## F-BRW-01 — fixed

**Root cause (corrected diagnosis).** The prior "self-calibrating" fix never engaged live. Added
temporary `eprintln!` instrumentation to `NativeWebViewElement::prepaint` and drove the app on the
X11 lane:

```
[F-BRW-01 DEBUG] element bounds=Bounds { origin: Point { x: 330.85715px, y: 114.00001px },
  size: Size { 728.5715px × 678.8572px } } window.scale_factor=1.1666666
  window.bounds=Bounds { origin: Point { x: 739.7143px, y: 198.85715px },
  size: Size { 1470px × 833.1429px } }
[F-BRW-01 DEBUG] requested=(728.5714721679688,678.857177734375) actual=(729,679)
```

`webview.bounds()` (real `XGetWindowAttributes` readback) matched the *requested* rect
byte-for-byte — wry places the X11 child exactly where asked and applies no scaling of its own on
this build. So the calibration loop always converged to `factor = 1.0` and never corrected anything.
The real defect is one level up: GPUI hands `prepaint` a `Bounds<Pixels>` that is already
`1/window.scale_factor()` smaller than physical screen pixels (`window.scale_factor() == 1.1666666`
on this desktop) — confirmed against the D-P1 critic's own pair (requested 850×792 at 386,133,
observed 728×679 at 331,114 — exactly `850/1.1667`, `792/1.1667`, `386/1.1667`, `133/1.1667`).

**Fix.** `native_webview_rect(bounds, scale_factor)` now multiplies the incoming layout bounds by
`window.scale_factor()` before building wry's `Rect` (still tagged `Logical`, since wry's own
`to_logical` is a no-op on an already-`Logical` value and passes the number straight to the X11
`resize`/`move_` calls). The old self-calibration loop is kept as a residual-error safety net (it is
now a no-op — factor converges to 1.0 immediately since the direct correction already lands exactly).
Removed the diagnostic `eprintln!`s after confirming the fix.

**Files changed:** `rust/crates/tiller_ui/src/browser.rs`
**Commit:** `4a6d36c fix(browser): recover physical webview bounds from GPUI's scale factor (F-BRW-01)`

**Tests:** `cargo test -p tiller_ui browser::` — 12/12 pass, including two rewritten/new unit tests
(`webview_bounds_recover_physical_target_from_live_fractional_scale_factor` uses the exact live
numbers above; `webview_bounds_reject_non_finite_scale_factor` covers the NaN/zero guard).

**Live verification (X11 lane):**

```bash
export TILLER_SOCKET=/tmp/<label>.sock TILLER_DB=/tmp/<label>.sqlite
Scripts/linux-drive.sh out.png '
  python3 Scripts/control-probe.py "$TILLER_SOCKET" project.add path=<repo>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" workspace.select workspace=<id from workspace.list>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.open url=https://example.com
  sleep 5
'
```

Screenshot before the fix (`/tmp/g1brw_out2.png`, discarded — not committed) showed the white
webview content confined to roughly x=331..1060 inside a chrome/toolbar span of x=386..1236 —
matching D-P1's numbers exactly. After the fix (same recipe, rebuilt binary) the webview fills the
entire Browser panel, flush with the toolbar left/right edges and the tab strip below it, with no
visible dark gap on any side.

**How to re-exercise:** run the recipe above (X11 lane only — Wayland cannot construct the webview
at all, see P127) and open the resulting PNG; the webview's white content area must span the full
width between the left sidebar and the right Files panel, and its top/bottom must be flush with the
toolbar and window edges — no shrink-toward-origin gap.

---

## F-CTRL-BROWSER-05 — already-correct, no code change

**Latest evidence before this row:** `half-proven` — two independent X11-lane runs returned
`Browser child is unavailable` and `loading:true` indefinitely.

**Re-verified at current HEAD (post the P126 dispatch-timeout fix and the F-BRW-01 geometry fix,
same binary build)**, following the exact recipe from `docs/linux-rewrite/tasks/P127-…`
(`workspace.select workspace=<id>`, not `path=`; `env -u WAYLAND_DISPLAY` via `linux-drive.sh`):

```
{"id":"probe","ok":true,"result":{"surface":"surface:2","title":"","url":"https://example.com"}}
{"id":"probe","ok":true,"result":{"canGoBack":"false","canGoForward":"false","error":"",
  "loading":"false","title":"Example Domain","url":"https://example.com/"}}
```

`loading` flips to `false` and `title` resolves to `"Example Domain"` — exactly the observation this
row requires. Reproduced across two separate fresh launches (`/tmp/g1brw_out2.png` and
`/tmp/g1brw_final2.png` runs, different sockets/DBs). The prior `half-proven` verdict was recorded
under the Wayland-lane instrument confusion described in P127, not a real defect. No code change was
needed or made for this row.

**Files touched:** none (owned files re-read, no diff required for this row specifically; the P126
fix in `rust/crates/tiller/src/main.rs` — committed separately — was a prerequisite that removed one
source of false negatives for this row but is not itself part of "fixing" BROWSER-05).

**How to re-exercise:**

```bash
export TILLER_SOCKET=/tmp/<label>.sock TILLER_DB=/tmp/<label>.sqlite
Scripts/linux-drive.sh out.png '
  python3 Scripts/control-probe.py "$TILLER_SOCKET" project.add path=<repo>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" workspace.select workspace=<id from workspace.list>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.open url=https://example.com
  sleep 5
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.get
'
```

Expect `{"loading":"false","title":"Example Domain", ...}`. Must run with `WAYLAND_DISPLAY` unset
(`linux-drive.sh` does this automatically) — on Wayland this row is genuinely unreachable (P127), not
evidence of a defect.

---

## F-CTRL-BROWSER-06 — already-correct, no code change

**Latest evidence before this row:** `half-proven` — `browser.eval script=document.title` returned
`Browser child is unavailable` or a 5s dispatch-timeout artifact.

**Re-verified at current HEAD**, same run as F-CTRL-BROWSER-05 above:

```
{"id":"probe","ok":true,"result":{"result":"\"Example Domain\""}}
```

`browser.eval script=document.title` returns real page content, not the "unavailable" error and not a
timeout artifact. Reproduced in the same two independent runs as BROWSER-05. No code change was
needed or made for this row.

**Files touched:** none.

**How to re-exercise:** same recipe as F-CTRL-BROWSER-05, add:

```bash
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.eval script=document.title
```

Expect `{"result":"\"Example Domain\""}`.

---

## Unrelated fix landed in this slice (prerequisite noted by the brief, P126)

`rust/crates/tiller/src/main.rs`: `queue_action`'s dispatch timeout (`CONTROL_ACTION_TIMEOUT`, a
hardcoded 5s) previously truncated any control call with a caller-supplied `timeoutMs` above ~5s
(e.g. `browser.wait`) and reported the misleading `"control action timed out"`. It now derives the
dispatch bound from the request's own `timeoutMs` (plus a 2s margin) when present, and the timeout
error text now distinguishes "the dispatch bound fired" from "the awaited condition never occurred".
Commit: `9ed8f08 fix(control): derive dispatch timeout from caller timeoutMs (P126)`.

**How to re-exercise:** `python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.wait timeoutMs=15000`
against a slow/never-loading URL should now wait ~15s (not truncate at 5s) before returning, and a
genuine dispatch stall (not exercised here) would say "dispatch bound … fired" rather than the old
ambiguous message.

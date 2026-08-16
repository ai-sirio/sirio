# Wave G slice G1-browser — critic verdicts

Independently re-verified at HEAD (`c1ecc7c`) with the app's own binary (`rust/target/debug/tiller`,
built 2026-08-16 11:33, after HEAD), driven live on the X11 lane (`env -u WAYLAND_DISPLAY`,
`DISPLAY=:1`) via `Scripts/linux-drive.sh` — not by trusting the brief's pre-filled critic evidence
or the builder's report. Two independent fresh launches (fresh `TILLER_SOCKET`/`TILLER_DB` each
time, app process killed and relaunched between them), both driving `project.add` -> `workspace.list`
-> `workspace.select workspace=<id>` -> `browser.open url=https://example.com` -> sleep 5 ->
`browser.get` / `browser.eval script=document.title`, produced byte-identical results.

## F-BRW-01 — PASSED

Both runs' screenshots (`/tmp/g1crit/out3.png`, `/tmp/g1crit2/out.png`) were pixel-scanned with
ImageMagick (`convert … txt:-`), reproducing the D-P1 critic's own methodology. Horizontal scans at
y=400 and y=850/y=500 across both runs: contiguous near-white (webview content, RGB ~238) span
x=386..1235, 850px wide with zero gaps — exactly the toolbar's own chrome span from the prior
FAILED evidence (x=386..1236), not the shrunk x=331..1060 span the defective build produced.
Vertical scan at x=800: near-white span y=133..924, flush with the toolbar bottom and the window's
lower edge. The webview now fills the panel exactly; no shrink-toward-origin. `cargo test -p
tiller_ui browser::` — 12/12 pass, including the two rewritten tests
(`webview_bounds_recover_physical_target_from_live_fractional_scale_factor`,
`webview_bounds_reject_non_finite_scale_factor`). This is discriminating: a still-broken build would
reproduce the old x=331..1060 span, not the toolbar-flush one measured here.

## F-CTRL-BROWSER-05 — PASSED

Both independent runs' `browser.get` returned
`{"canGoBack":"false","canGoForward":"false","error":"","loading":"false","title":"Example
Domain","url":"https://example.com/"}` — `loading:false` and `title:"Example Domain"` exactly as the
row requires, not the default `loading:true`/`title:""` a never-loaded or never-constructed webview
would show. Confirms the builder's already-correct claim; the prior half-proven verdict was the
Wayland-lane / missing-`workspace.select` instrument confusion described in P127, not a live defect.

## F-CTRL-BROWSER-06 — PASSED

Both independent runs' `browser.eval script=document.title` returned
`{"result":"\"Example Domain\""}` — real evaluated page content, not `'Browser child is
unavailable'` and not a dispatch-timeout artifact. Confirms the builder's already-correct claim on
the same two live runs used for BROWSER-05.

**Instrument note:** all three rows were exercised through one combined Python driver
(`/tmp/g1crit/drive.py`, not committed — a throwaway harness, not part of the record) that opens the
control socket directly and issues `project.add` / `workspace.list` (parsing the nested JSON-string
`workspaces` field) / `workspace.select` / `browser.open` / `browser.get` / `browser.eval` in
sequence, run inside `Scripts/linux-drive.sh`'s action block so the app launches on the X11 lane
under the script's drive lock. Re-exercise with the recipe in `G1-browser-report.md`.

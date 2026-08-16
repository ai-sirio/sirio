# P133 — GPUI drops an XDND payload entirely when the source resolves slowly

**Upstream defect, not fixable in this tree.** Holds `F-CORE-FILE-03A` at `FAILED — defective`.

Found by the wave-I `I2-xdnd` critic using the `wl_data_device_manager` drag-source client built in the
same wave (`Scripts/xdnd-source/`), which is the first instrument in this project able to deliver a
real compositor-mediated XDND drop.

## What happens

With an ordinary-speed drag, the app is correct. Two files dropped on a terminal pane land in the
prompt **in the user's drop order**, through the production `on_drop::<ExternalPaths>` handler:

```
'/tmp/critic-xdnd-alpha.txt' '/tmp/critic-xdnd-beta.txt'
```

With `--delay-ms 400` — the contract's own *"a provider that resolves slowly"* clause — the drop is
**silently and totally lost**. `DROP_PERFORMED` fires, the source times out waiting for
`dnd_finished`/`dnd_cancelled`, and the prompt stays empty. Nothing is logged and nothing is shown to
the user.

Note this is a *stronger* failure than the one the contract was written to catch. The row asks whether
a slow provider can **reorder** the files; the answer is that a slow provider loses all of them.

## Root cause

In the vendored pinned `zed-industries/zed` checkout (rev `c05e346`), `gpui_linux`'s Wayland
`wl_data_device` handling reads `text/uri-list` from the offer pipe in a **background task started on
`Enter`**. The `Drop` handler bails out unless that async read has already completed. When the source
delays its pipe write, `Drop` arrives roughly 150–350 ms before the read finishes, so the handler
takes its early-return path and the payload is discarded.

The race is entirely inside GPUI. **No file this repository owns is involved** — our side
(`tiller_terminal/src/lib.rs`'s `.on_drop::<gpui::ExternalPaths>` and
`tiller_project::terminal_file_drop`) was independently traced correct and is order-preserving by
construction, and a pre-existing test drives a real two-file ordered drop through GPUI's own mouse
dispatch and passes.

## Why it was not fixed here

Fixing it means patching GPUI, which this repo consumes as a pinned git rev. That means forking
`zed-industries/zed` and carrying a `[patch]` entry — a standing maintenance commitment on a
409-crate dependency, taken on for one row. That is a real decision with real ongoing cost and it
belongs to a human, not to a wave closing out its last verdicts.

The shape of the fix, for whoever takes it: `Drop` should **await** the in-flight `Enter` read rather
than early-returning when it has not finished, or the read should be started eagerly enough that
`Drop` can rely on it. Either way the failure mode should become visible — a discarded drop that logs
nothing is what let this survive four waves of testing.

## Why it took four waves to find

Every earlier pass judged this row by reading code or by GPUI-internal simulated drags, both of which
exercise the path *after* the payload has been resolved. The bug lives in the resolution step itself,
so no amount of code-reading or in-process testing could reach it. It took a real compositor-mediated
drop from a genuine Wayland data source — which is exactly why the instrument was worth building even
though the row it was built for ended up failing.

## Related

`F-CORE-FILE-03A` in `INVENTORY-LEDGER.md`. The instrument is `Scripts/xdnd-source/`, documented in
`docs/linux-rewrite/WAYLAND-LANE.md`; reproduce with `--delay-ms 400` against a terminal pane.

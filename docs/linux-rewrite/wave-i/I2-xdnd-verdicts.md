# I2-xdnd verdicts

## F-CORE-FILE-03A — FAILED — defective

Independently reproduced both halves of the row's own VERIFY clause with my own file names, not
the builder's, in two separate `wayland-drive.sh` invocations against a freshly rebuilt
`Scripts/xdnd-source`:

- `xdnd 6 6 700 400 /tmp/critic-xdnd-alpha.txt /tmp/critic-xdnd-beta.txt` (ordinary speed) — the
  full real protocol sequence fired (`DRAG_STARTED` -> `TARGET text/uri-list` -> `ACTION Copy` ->
  `SEND` -> `DROP_PERFORMED` -> `FINISHED`) and the Terminal pane's prompt genuinely showed
  `'/tmp/critic-xdnd-alpha.txt' '/tmp/critic-xdnd-beta.txt'`, in order, through the production
  `on_drop::<gpui::ExternalPaths>` path — confirms the ordinary case really works via a real
  compositor-delivered XDND drop, not a substituted test.
- `xdnd 6 6 700 400 /tmp/critic-xdnd-slow.txt --delay-ms 400` (the row's own "provider that
  resolves slowly" clause) — `DROP_PERFORMED` fired but `xdnd-source` then timed out after 15s
  waiting for `dnd_finished`/`cancelled`, and the Terminal pane's prompt stayed completely empty:
  the drop was silently and totally lost, not merely reordered.

The row's contract text requires that a slow-resolving provider still "preserve order" on
"classification and insertion." Losing the drop entirely is a stronger failure than a reordering
bug, and I reproduced it myself with a file the builder never used, so this is not a transcribed
claim. The root cause (`wl_data_device::Event::Drop`'s handler bails out unless `Enter`'s async
pipe-read task already populated `state.drag.window`, and the async read only starts on `Enter`,
racing the scripted `Drop` by roughly 150-350ms in this drive) sits in the vendored, pinned
`zed-industries/zed` `gpui_linux` checkout, not in any file this repository owns — so there is
nothing to fix here, but the row is exercised and it fails its own VERIFY clause for the
slow-provider case. The builder's "half-proven" self-grade undersells this: the ordinary path is
now a genuine `PASSED`-quality instrument, but the slow-provider clause is a reproduced, live
defect, which is a stronger and more specific finding than "not yet proven."

The new instrument itself (`Scripts/xdnd-source`, a real `wl_data_device_manager` drag-source
client, wired into `wayland-drive.sh`'s new `xdnd` action and documented in
`docs/linux-rewrite/WAYLAND-LANE.md`) is real and reusable — it is what let this defect be found
at all, and is not itself in question.

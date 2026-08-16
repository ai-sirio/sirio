# I2-xdnd report

## F-CORE-FILE-03A — half-proven for lack of a real compositor-delivered XDND drop

**Action:** implemented (the missing instrument), plus one real defect found and fixed
inside that instrument, plus one real defect found and recorded (not fixed — lives in a
pinned upstream git dependency, not in this repo).

**Diagnosis confirmed at HEAD.** The recorded diagnosis (`sweep H5-drive`, 2026-08-16)
was re-verified unchanged: `wl_data_device` `Enter` in the vendored pinned Zed
`gpui_linux` checkout (`crates/gpui_linux/src/linux/wayland/client.rs`) reads
`text/uri-list` through one pipe in one background task and collects with
`.lines().filter_map(Url::parse)` into a `SmallVec`; `Drop` carries only a position;
`tiller_terminal/src/lib.rs`'s `on_drop::<gpui::ExternalPaths>` (now at line 1708) calls
`paths().to_vec()`; `tiller_project::terminal_file_drop` is an order-preserving
quoted-and-space-joined map/join. The pre-existing test
`a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files` still passes
and still only exercises GPUI's own internal simulated-drag harness — not a real
compositor drag. Nothing here needed changing; what was missing was the instrument.

**Built:** `Scripts/xdnd-source` — a new, standalone Rust crate (own `Cargo.toml`,
`Cargo.lock`, `.gitignore`; not a member of `rust/`'s workspace, so it never touches the
app's own `Cargo.lock` or build). It uses `wayland-client` 0.31 + `wayland-protocols-wlr`
0.3 (both already present in the local registry cache) to become a real
`wl_data_device_manager` drag **source** on the same nested-sway `WAYLAND_DISPLAY`
`wayland-drive.sh` already drives:

- Maps a tiny (10×10px) `zwlr_layer_shell_v1` **overlay-layer** surface — deliberately not
  an `xdg_toplevel`, so sway's tiling never resizes or reflows Tiller's own window or
  shifts the coordinate space every other `wayland-drive.sh` action already depends on.
- Prints `READY` once that surface is configured and eligible for pointer focus.
- `wayland-drive.sh`'s new `xdnd` action then presses the **same persistent virtual
  pointer** `click`/`drag`/`modclick` already use (proven live in P124/P130) at the
  overlay's position — a real `wl_pointer.button` press the compositor delivers, which is
  the only legitimate source of the serial `wl_data_device.start_drag` requires (a serial
  cannot be forged from an unrelated process).
- `xdnd-source` prints `DRAG_STARTED` once `start_drag` (offering `text/uri-list`) is
  sent; the driver walks real intermediate `move` waypoints to the drop target and
  releases; the compositor delivers `wl_data_device.enter`/`motion`/`drop` to whatever
  surface is now under the pointer — Tiller's own window.

**Proven live, ordinary speed.** Two files
(`/tmp/xdnd-test-a.txt`, `/tmp/xdnd-test-b.txt`) dragged onto a running Terminal pane
landed, in order, shell-quoted and space-joined, in the pane's real prompt —
`'/tmp/xdnd-test-a.txt' '/tmp/xdnd-test-b.txt'` — exactly `tiller_project
::terminal_file_drop`'s output shape, reached through the production
`on_drop::<gpui::ExternalPaths>` handler, not a test fixture. `xdnd-source` reported the
full protocol sequence: `DRAG_STARTED` → `TARGET text/uri-list` → `ACTION Copy` →
`SEND` → `DROP_PERFORMED` → `FINISHED`. Screenshots:
`reference/linux-progress/p132-xdnd/01-before-real-xdnd-drop.png` and
`02-after-real-xdnd-drop-two-files.png`.

**A real bug this instrument found and fixed in itself.** The first working version
printed `DRAG_STARTED` then `CANCELLED` within tens of milliseconds — before the driver
had sent a single `move` waypoint. `WAYLAND_DEBUG=1` on `xdnd-source`'s own connection
showed why: the pointer is still over `xdnd-source`'s own overlay for the first instant
of every drag, so the compositor self-delivers a `data_offer`+`enter` to `xdnd-source`'s
own `wl_data_device`, treating it as a candidate target. The first version's
`Dispatch<WlDataDevice>` handler called `id.destroy()` on that self-offer immediately;
wlroots reads an unaccepted, destroyed offer on the *origin* surface as "no one will ever
take this drag" and cancels it — visible on the wire as `enter`/`motion` immediately
followed by `leave` + `wl_data_source.cancelled`. Fix: do nothing with the self-offer at
all, matching `gpui_linux`'s own `DataSourceKind::Drag` handler (which likewise treats
`dnd_finished` and a trailing `cancelled` as interchangeable teardown signals and keeps
whichever arrives first).

**The "slow-resolving provider" clause — simulated, and it is not the same hazard.** On
`text/uri-list` the whole list arrives through one pipe in one write — there is no
per-file async resolution to race, unlike macOS's `NSItemProvider`, which resolves each
dragged item independently. `xdnd-source --delay-ms N` simulates the closest analogue
this MIME type has: it delays the one pipe write by N ms after the target's `receive()`
triggers the `send` event. **This is not proven equivalent to the macOS clause** — it is
a single-shot delay on the only write, not a per-item race — and that judgment is left to
whoever reads this next.

**A second real defect this found, in the vendored checkout, not in this repo.** At
`--delay-ms 400` and `--delay-ms 2000` the drop is silently lost: no paths reach the
terminal prompt (`reference/linux-progress/p132-xdnd/03-slow-provider-400ms-drop-lost.png`,
prompt empty), the app logs no error, and `xdnd-source` itself receives neither
`dnd_finished` nor `cancelled` (times out at its own 15s safety net). This is well inside
the app's declared `PIPE_READ_TIMEOUT` (4s, `gpui_linux`'s `linux/platform.rs`), so that
timeout is not what fires. Root cause, read from `client.rs`: `wl_data_device
::Event::Drop`'s handler bails out immediately (`let Some(drag_window) = state.drag
.window.clone() else { return; };`) unless `Enter`'s **async** pipe-read task has
*already* completed and populated `state.drag.window` — and in this drive that task only
starts once `Enter` fires, roughly 150-350ms before the scripted button release
(`Drop`) reaches the app. The ordinary (`--delay-ms 0`) case wins that race comfortably;
a few hundred milliseconds of provider latency does not. Once `Drop` bails,
`data_offer.finish()`/`destroy()` are never called, so neither side is ever told the
transfer is over, and the paths are discarded even once the async read later succeeds,
because no further `Drop` event will ever arrive to consume the result. **Not fixed**:
this lives entirely in the vendored, pinned `zed-industries/zed` git dependency
(`crates/gpui_linux/src/linux/wayland/client.rs`), not in any file this repository owns
— recorded here rather than patched, and not filed as a `wantedForeignFiles` entry since
it isn't a file in this repository at all.

**Build:** `cd Scripts/xdnd-source && cargo build` — clean, zero warnings (verified after
`cargo clean` + rebuild). `cd rust && cargo build -p tiller` unaffected (pre-existing
warnings only, unrelated to this row). `cd rust && cargo test -p tiller_terminal
a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files` — still passes.

**Commit:** `29ac6885` — `feat(I2-xdnd): real wl_data_device_manager XDND drag source
for F-CORE-FILE-03A` — `Scripts/wayland-drive.sh`, `Scripts/xdnd-source/{.gitignore,
Cargo.toml,Cargo.lock,src/main.rs}`, `docs/linux-rewrite/WAYLAND-LANE.md`,
`reference/linux-progress/p132-xdnd/*.png`.

**howToExercise:**

```bash
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
printf 'a\n' > /tmp/xdnd-test-a.txt; printf 'b\n' > /tmp/xdnd-test-b.txt
TILLER_WL_LABEL=<unique> Scripts/wayland-drive.sh /tmp/xdnd-shots '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  xdnd 6 6 700 400 /tmp/xdnd-test-a.txt /tmp/xdnd-test-b.txt
  shot after-drop
'
```

The Terminal pane's shell prompt should visibly show
`'/tmp/xdnd-test-a.txt' '/tmp/xdnd-test-b.txt'` typed into it in the `after-drop`
capture — a real XDND drag landing through the production `on_drop::<gpui
::ExternalPaths>` path, not a socket call or an in-process test harness. Add
`--delay-ms 400` after the file list to reproduce the lost-drop defect (prompt stays
empty instead).

No `wantedForeignFiles`: everything fixed is owned by this slice
(`Scripts/wayland-drive.sh`, the new `Scripts/xdnd-source/`, `docs/linux-rewrite
/WAYLAND-LANE.md`). The one further defect found (the `Drop`-handler race) lives outside
this repository entirely, in the pinned upstream `gpui` git dependency, and is recorded
rather than requested as a foreign-file change.

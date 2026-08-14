# The Wayland lane — a second visual display, in parallel, with no drive lock

**The app renders fully headless under a nested Wayland compositor**, and `grim` captures real
pixels. This is a *second* display: it does not touch `DISPLAY=:1` and does not take the drive lock,
so several agents can each run their own instance at the same time.

`ENVIRONMENT.md` §"There is no headless critic" is about **X11** and remains true for X11. Xvfb and
Xephyr paint nothing because GPUI must present into an X drawable through DRI3, which they have no
path for. Wayland is a different mechanism entirely: the client hands a `wl_buffer` to the
compositor, and wlroots' headless backend composites it in software with no GPU involved. Same
lavapipe, same absent hardware, opposite outcome. **Do not conclude from the X11 result that this
one cannot work — it is verified working, with screenshots in `reference/linux-progress/`.**

## What it proves, and what it does not

| | |
|---|---|
| ✅ **The full app renders** | sidebar, tab bar, terminal with live PTY output, Files panel, status bar, traffic lights — all real pixels |
| ✅ **Visual comparison against the bar** | COSMIC/comet screenshots can be compared against a capture taken any time, by anyone, in parallel |
| ✅ **Socket-driven state is visible** | drive a change over the control socket, force a repaint, and see it — e.g. `project.add` populated the sidebar with `tiller`, its two worktrees, and the `Primary` badge |
| ✅ **Layout at any size** | change the output resolution and capture the reflow |
| ❌ **No synthetic input** | clicks and keystrokes **do not reach the client** — see trap 3. Anything gated on a pointer or keyboard gesture (context menus, drag-and-drop, typing) still needs `DISPLAY=:1` |

**A row whose `VERIFY` line names a click, a right-click, a drag or typed text cannot be closed
here.** Use the drive lock for those. A screenshot that merely *contains* a control is not proof the
control works — that is the same mistake that produced this project's false `PASSED`s.

## Setup

Use your own paths everywhere so instances do not collide. Keep sockets under `/tmp/` — the
`sockaddr_un` limit is **108 bytes** and the scratchpad path overflows it.

```bash
cat > /tmp/<you>-sway.conf <<'EOF'
default_border none
default_floating_border none
gaps inner 0
gaps outer 0
output HEADLESS-1 resolution 1715x972
EOF

env -u WAYLAND_DISPLAY -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/1000 \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    SWAYSOCK=/tmp/<you>-sway.sock \
    nohup sway -c /tmp/<you>-sway.conf > /tmp/<you>-sway.log 2>&1 &
```

**Read back which display it chose — never assume `wayland-1`:**

```bash
grep "Running compositor on wayland display" /tmp/<you>-sway.log
# -> Running compositor on wayland display 'wayland-2'
```

Then the app, with `DISPLAY` **unset** so GPUI selects its Wayland backend:

```bash
env -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/1000 \
    WAYLAND_DISPLAY=wayland-2 \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    TILLER_DB=/tmp/<you>.sqlite TILLER_SOCKET=/tmp/<you>.sock \
    nohup rust/target/debug/tiller > /tmp/<you>.log 2>&1 &
```

Capture:

```bash
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-2 SWAYSOCK=/tmp/<you>-sway.sock
swaymsg output HEADLESS-1 resolution 1400x900   # forces a repaint — see trap 2
grim -o HEADLESS-1 /path/to/shot.png
```

## Four traps

1. **`DISPLAY` must be unset for the app.** With `DISPLAY` set, GPUI takes the X11 path and you are
   back in the lane that paints nothing. `env -u DISPLAY`, not `DISPLAY=`.

2. **Repaint is lazy — a state change alone does not produce a new frame.** After the first frame the
   app sits still, and `grim` keeps returning that frame **byte for byte**. A `project.add` that
   demonstrably succeeded over the socket produced an identical MD5. Force a configure by setting the
   output to a resolution **different from the current one** — same value is a no-op — then capture.
   Alternate between two sizes across a session.

   **This is the trap most likely to manufacture a false verdict**, in either direction: an unchanged
   screenshot reads as "the feature did nothing" when the app simply never repainted. Verify a
   negative by forcing a repaint before you believe it.

3. **Synthetic input does not work, and fails silently.** `swaymsg seat seat0 cursor set/press`
   returns `success: true` and does nothing; `wtype` and `wlrctl` (both installed 2026-08-14) exit 0
   and nothing lands. The cause is in `swaymsg -t get_seats`: the seat reports
   **`capabilities: 0`, `devices: []`**. The headless backend creates no input devices, so sway never
   advertises `wl_pointer`/`wl_keyboard` to the client, and a transient virtual device from `wtype`
   is created and destroyed faster than the client can bind and take focus. Priming with a
   long-lived `wtype -d 3500 -k Shift_L …` in the background does not fix it either — that was
   tested.

   **Every input tool here reports success while doing nothing.** Never infer that a gesture landed;
   confirm with a forced-repaint capture, and if the frame is unchanged, the gesture did not happen.

4. **Clean up by matching on the environment, never on the process name.** `pkill sway` or
   `pkill tiller` will kill other agents' instances, and on a machine where the user runs a Wayland
   session it can kill theirs:

   ```bash
   for p in $(pgrep -x tiller); do
     tr '\0' '\n' < /proc/$p/environ | grep -q '^TILLER_SOCKET=/tmp/<you>.sock' && kill $p
   done
   for p in $(pgrep -x sway); do
     tr '\0' '\n' < /proc/$p/environ | grep -q '^SWAYSOCK=/tmp/<you>-sway.sock' && kill $p
   done
   ```

## Noise you should ignore

These appear on every launch and are harmless — Mesa probing for a GL path that is not there. GPUI
renders through blade/Vulkan, so the probe failing costs nothing:

```
libEGL warning: failed to get driver name for fd -1
libEGL warning: MESA-LOADER: failed to retrieve device information
MESA: error: ZINK: failed to choose pdev
libEGL warning: egl: failed to create dri2 screen
```

A log full of errors beside a perfectly rendered frame is exactly what makes an agent abandon a
working lane. Check the capture before you believe the log.

## Combining the lanes

- **`HEADLESS-LANE.md`** — the control socket, no rendering at all. Best for state and API rows.
- **This lane** — the socket *plus* real pixels. Best for anything whose evidence is visual.
- **`DISPLAY=:1` + the drive lock** — the only lane with working input. Reserve it for gestures.

The natural division: everything that can be driven by socket and judged by eye moves here and runs
in parallel; the drive lock is spent only on clicks, keystrokes and drags.

## Verified working, 2026-08-14

Launched under `sway` 1.10-headless with `WLR_RENDERER=pixman`; the app created a real
`xdg_shell` toplevel (`sway/desktop/xdg_shell.c: New xdg_shell toplevel`) and appeared in
`swaymsg -t get_tree` as `con id=5 focused=True visible=True`. `grim` captured the complete UI. A
`project.add` over the control socket then appeared in the sidebar after a forced repaint —
`tiller`, `rust/gpui-rewrite` badged `Primary`, `linux/gpui-waku` with its `Chat` and `Terminal`
tabs, and `New Worktree…`.

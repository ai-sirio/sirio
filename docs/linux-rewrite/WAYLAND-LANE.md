# The Wayland lane — a second visual display with synthetic input, in parallel, with no drive lock

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
| ✅ **Surfaces open and navigate over the socket** | `surface.settings.open`/`.select`, `surface.changes.open`, `browser.open`, `project.add`, `panel.*`, `tab.*`, `pane.*` — see "Driving without a pointer" below |
| ✅ **Chat turns, in the visible transcript** | since `P107` (2026-08-14) `surface.chat.compose`/`.send` drive the rendered chat, so a real ACP turn can be photographed here. Socket-sourced `F-CHAT` evidence dated **before** that commit proves the old invisible replica and must be re-driven |
| ✅ **Synthetic left-click, text and named-key input** | `click`, `move`, `type` and `key` in `wayland-drive.sh` use persistent virtual devices created **before** Tiller connects — see trap 3 and P112. Pointer drags, right-click, modifiers/chords and IME/non-ASCII text are not yet exercised. |
| ❌ **No webview content** | the embedded browser needs an X11 window handle and gets a Wayland one; its chrome renders, the page does not. Every `F-BRW` row belongs on `DISPLAY=:1` |

**A row whose `VERIFY` line names an absolute move, left-click, ASCII typed text, or named key can
be closed here** when `move`/`click`/`type`/`key` drives that exact gesture and a forced-repaint
capture shows its result. Right-click, button-held drag, modifier chords (including `Shift+Tab`),
non-ASCII text, and IME input still require `DISPLAY=:1` until separately proven. A screenshot that
merely *contains* a control is not proof the control works — that is the same mistake that produced
this project's false `PASSED`s.

## Driving without a pointer — what this lane can actually close

Verified 2026-08-14 (afternoon). `system.capabilities` advertises **54 methods**, and the ones that
move a *visible surface* turn this lane into a real second drive lane — no lock, no display, any
number of agents in parallel:

```bash
python3 ctl.py surface.settings.open                    # → Settings renders, full state returned
python3 ctl.py surface.settings.select section=general  # → section switches, state returned
python3 ctl.py surface.changes.open                     # → Changes surface, changed[] + loading
python3 ctl.py browser.open url=https://example.com     # → ok, returns surface:N
python3 ctl.py project.add path=/abs/path               # → sidebar populates
```

The capture then shows the result. One round trip proved it: `surface.settings.open` followed by a
forced repaint rendered Appearance in full — the System/Light/Dark segmented control, the
Translucency toggle, Interface and Terminal font sizes, File icons, and the five Agent Colors rows.

**And here is the line you must not cross.** Driving a surface over the socket proves **the handler
and the render**. It does **not** prove the gesture. A row whose clause says *click the Light
segment* is not closed by a socket call that switches the theme — that is precisely the
"declared path passes for a control nobody can reach" failure this project has already paid for.
Socket-driving closes a row's **state half**; the gesture half stays owed and stays `half-proven`.
Say which half you drove, every time.

### `open` does not focus — `tab.select` does

Both observations this document previously handed off are settled, and they were the same thing:

- `surface.chat.open` returned `error: "no current workspace"` only because the instance had no
  project. `project.add path=<abs>` creates and selects one (`workspace.current` then answers), and
  `surface.chat.open` succeeds. Not a defect.
- `surface.chat.open` and `browser.open` both return `ok` with full surface state while **the
  visible tab does not change**. That is not a "socket that says yes": the surface really is
  created, it is placed in the tab strip, and it simply is not brought forward. `tab.select`
  brings it forward and the capture then shows it.

**`tab.select` is 1-based.** `index=0` is refused with `tab.select index must be a positive
integer`, and every param is a string — `index=0` sent as a JSON integer is rejected earlier still
with `invalid type: integer 0, expected a string`. Note that the Rust builder
`request::tab_select(position: usize)` documents no such base, so `tab_select(0)` always fails at
runtime.

### `surface.chat.*` drives the rendered chat — fixed 2026-08-14, was not always true

**This lane can now judge `F-CHAT` content.** `P107` (commit `6a6f558`) routes chat control requests
onto GPUI and mutates the mounted `Chat` entity; the invisible replica is gone. Verified by capture,
not by test: `reference/linux-progress/p107-rendered-transcript.png` shows a socket-driven turn
appearing in the visible transcript — the user message, the `Execute pwd` tool call marked
`Completed`, its output, and the assistant's reply — and `p107-composer-marker.png` shows
`surface.chat.compose` text in the visible composer.

**Evidence recorded before that commit is not retroactively valid.** Until `P107`, the chat API
operated on a `chat_sessions` map owned by the control handler (`tiller/src/main.rs:650`), disjoint
from the rendered view — its own ACP agent, its own database. A completed turn with a real assistant
answer left the rendered transcript empty. It was convincing because the id was right:
`surface.chat.open` returned `surfaceId: "default-chat"`, genuinely the persisted id of the visible
Chat tab (`session.rs:322`), so the socket answered with the right name for the wrong object. Any
`F-CHAT` verdict sourced from a socket drive dated before 2026-08-14 evidences the old replica and
must be re-driven — that is why the whole family is marked owed in `P106-report.md`.

`tab.select` is still what brings the Chat tab forward; `surface.chat.open` does not focus.

### The embedded browser does not work on this lane

`browser.open` + `tab.select` reaches the Browser surface, and its chrome renders — address bar,
back/forward/stop, the title readout. The **page** does not. The webview needs an X11 window handle
and GPUI hands it a Wayland one, so the content area shows:

```
Direct XCB build failed: the window handle kind is not supported; XCB->Xlib adapter failed:
GPUI returned unsupported handle: Wayland(WaylandWindowHandle { surface: 0x... })
```

This is a property of the lane, not a defect in the browser: the same URL renders its real content
on `DISPLAY=:1` (`reference/linux-progress/p86-post-browser.png` shows example.com's actual page).
**Route every `F-BRW` row to the X lane.** Browser *chrome* can be judged here; browser *content*
cannot.

## Setup

**`Scripts/wayland-drive.sh` does all of this for you** — boot, drive, capture, tear down — and is
the way to use this lane:

```bash
Scripts/wayland-drive.sh /tmp/my-shots '
  ctl surface.settings.open
  shot settings-appearance
  ctl surface.settings.select section=general
  shot settings-general
'

# A real, socket-unreachable gesture: focus the Projects filter, type, then prove the frame changed.
Scripts/wayland-drive.sh /tmp/my-input-shots '
  click 100 80
  type WAYLAND_INPUT_PROOF
  key BackSpace
  shot filter-input
'
```

Inside the snippet, `ctl <method> [k=v …]` sends a `ControlRequest` and prints the reply, and
`shot <name>` forces a repaint before capturing (trap 2, below) and refuses a blank frame. Set
`TILLER_WL_LABEL` to name your instance — every path derives from it, so two agents never collide —
and `TILLER_WL_KEEP=1` to leave the instance up for follow-up calls.

The rest of this section is what the script does, for when you need to do it by hand.

Use your own paths everywhere so instances do not collide. Keep sockets under `/tmp/` — the
`sockaddr_un` limit is **108 bytes** and the scratchpad path overflows it.

```bash
cat > /tmp/<you>-sway.conf <<'EOF'
xwayland disable
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
    nohup sway -d -c /tmp/<you>-sway.conf > /tmp/<you>-sway.log 2>&1 &
```

`xwayland disable` matters for parallelism: Xwayland claims a global `/tmp/.X11-unix/XN`, and a
second instance loses that race and **refuses to start at all** (`Failed to bind socket
@/tmp/.X11-unix/X2: Address already in use`). The app is a native Wayland client here and needs
none of it.

**Read back which display it chose — never assume `wayland-1`:**

```bash
grep "Running compositor on wayland display" /tmp/<you>-sway.log
# -> Running compositor on wayland display 'wayland-3'
```

**`-d` is load-bearing, not diagnostic noise.** That is an `INFO` line, and at sway's default level
the log carries errors only — without `-d` the grep silently matches nothing and you have no way to
learn your display. (An earlier version of this document omitted the flag, which made the whole
setup unreproducible.) sway skips any `wayland-N` whose lockfile is held, so parallel instances
land on different numbers by themselves.

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

## Five traps

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

3. **One-shot virtual devices race the client; persistent ones work.** A bare headless seat starts
   as **`capabilities: 0, devices: []`**. `wtype` then causes sway to advertise keyboard capability
   `2`; GPUI immediately creates `wl_keyboard` and receives every traced key event. `wlrctl pointer
   move`/`click` similarly causes capability `1`, but each one-shot process sends its event before
   GPUI can bind the newly advertised `wl_pointer`, so the client sees no `enter`, `motion` or
   `button`. Exit 0 meant the compositor accepted the request, not that its delivery race was won.

   `wayland-drive.sh` now starts a tiny virtual-pointer client and a quiescent long-lived `wtype`
   **before launching Tiller**. The pointer client remains connected and accepts `move`/`click`
   commands over a per-instance FIFO; the keyboard keeper has pressed and released Shift before
   Tiller starts, then stays alive without a modifier held. GPUI consequently binds both objects at
   startup. `click`/`move` and `type`/`key` verify `WAYLAND_DISPLAY` and the nested sway config with
   `swaymsg -t get_version` before injecting, so they cannot inherit the operator desktop.

   **Evidence, 2026-08-14 P112:** a persistent pointer produced `wl_pointer.enter`, `motion`, and
   left-button press/release in `WAYLAND_DEBUG=1`; `click 100 80`, `type P112_SCRIPT_E2E`, and a
   forced-repaint `shot` visibly populated the Projects filter. `key BackSpace` visibly removed the
   final `X` from `P112_NAMED_KEY_X`. See `P112-report.md`. Do not declare a gesture landed from an
   exit code: force a repaint and capture its visible result. Right-click, drag, modifier/chord,
   non-ASCII and IME paths remain unexercised.

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

5. **`grim` follows `WAYLAND_DISPLAY`, and yours is already set to the operator's desktop.** On this
   machine the login session exports `WAYLAND_DISPLAY=wayland-1`. Run `grim -o HEADLESS-1 shot.png`
   without overriding it and grim looks for `HEADLESS-1` on the **user's real compositor**, finds no
   such output, and writes **no file at all** — which then reads as "the app rendered nothing".
   Export `WAYLAND_DISPLAY=<your display>` in every shell that captures. On a compositor that did
   happen to have a matching output name, the same mistake photographs the operator's screen.

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
- **This lane** — the socket, real pixels, and verified left-click/text/named-key input. Best for
  visual and ordinary gesture rows; it remains parallel and lock-free.
- **`DISPLAY=:1` + the drive lock** — reserve for unexercised gestures (right-click, drag,
  modifiers/chords, non-ASCII/IME) and X11-only browser content.

The natural division: everything that can be driven by socket and judged by eye moves here and runs
in parallel; the drive lock is spent only on clicks, keystrokes and drags.

## Verified working, 2026-08-14

Launched under `sway` 1.10-headless with `WLR_RENDERER=pixman`; the app created a real
`xdg_shell` toplevel (`sway/desktop/xdg_shell.c: New xdg_shell toplevel`) and appeared in
`swaymsg -t get_tree` as `con id=5 focused=True visible=True`. `grim` captured the complete UI. A
`project.add` over the control socket then appeared in the sidebar after a forced repaint —
`tiller`, `rust/gpui-rewrite` badged `Primary`, `linux/gpui-waku` with its `Chat` and `Terminal`
tabs, and `New Worktree…`.

## A text-only agent can assert on a frame it cannot see — added 2026-08-14 18:10

You cannot look at a capture. You **can** measure one, and for the commonest rendering defect in
this project — a region that draws nothing — the measurement is decisive.

```bash
convert <png> -crop <W>x<H>+<X>+<Y> +repage -colorspace Gray \
        -format "%[fx:standard_deviation*255] %[fx:mean*255]" info:-
```

**Standard deviation is zero if and only if every pixel in that rectangle is identical.** A drawn
row, any glyph, any divider makes it non-zero. This is how the clipped Changes list was confirmed:
the region below the list measured `0.00` at two resolutions after a 12-second settle, while the
Files panel over the identical rows measured `22.0`.

Three rules make it evidence rather than a number:

1. **Always measure a positive control** — the same size region, same rows, somewhere content is
   known to be drawn. Without it, `stddev 0` could equally mean your crop coordinates were wrong or
   you photographed the wrong window. A live control proves the detector is awake.
2. **It proves absence far better than presence.** `stddev 0` is near-conclusive that *nothing* is
   drawn. A non-zero result proves only that *something* is drawn — never that the **right** thing
   is drawn. Never close a row on "the region is non-empty"; that is the same inference that
   produced this project's false `PASSED`s.
3. **Settle first, and check two resolutions.** `shot` alternates the output size to force a
   repaint, so a single capture can catch a mid-relayout frame. A defect that reproduces at both
   sizes after a settle is not an artifact.

`convert` and `identify` are installed; Python has **no** `PIL` and no `numpy` on this machine.

Use this to check your own work before asking for a visual acceptance pass — it turns a
frame you cannot read into a claim you can defend.

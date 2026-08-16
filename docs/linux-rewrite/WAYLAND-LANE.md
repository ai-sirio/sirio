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
| ✅ **Synthetic left-click, text and named-key input** | `click`, `move`, `type` and `key` in `wayland-drive.sh` use persistent virtual devices created **before** Tiller connects — see trap 3 and P112. |
| ✅ **Right-click, button-held drag, scroll, modifier chords** | `rightclick`, `down`/`up`/`drag` and `scroll` extend the same persistent virtual-pointer client; `chord` and `modclick` extend the same persistent virtual-keyboard path. See "New pointer and keyboard primitives, P124" below — each has a live positive-control capture. IME and non-ASCII text are still not exercised. |
| ❌ **No webview content** | the embedded browser needs an X11 window handle and gets a Wayland one; its chrome renders, the page does not. Every `F-BRW` row belongs on `DISPLAY=:1` |

**A row whose `VERIFY` line names an absolute move, left-click, right-click, button-held drag,
scroll/axis, modifier chord (including `Shift+Tab`/`Shift+F10`), modifier-held click, ASCII typed
text, or named key can be closed here** when the matching action in `wayland-drive.sh` drives that
exact gesture and a forced-repaint capture shows its result. Non-ASCII text and IME input still
require `DISPLAY=:1` until separately proven. A screenshot that merely *contains* a control is not
proof the control works — that is the same mistake that produced this project's false `PASSED`s.

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
- **This lane** — the socket, real pixels, and verified left-click/right-click/drag/scroll/
  chord/text/named-key input. Best for visual and ordinary gesture rows; it remains parallel and
  lock-free.
- **`DISPLAY=:1` + the drive lock** — reserve for still-unexercised gestures (non-ASCII/IME text)
  and X11-only browser content.

The natural division: everything that can be driven by socket and judged by eye moves here and runs
in parallel; the drive lock is spent only on non-ASCII/IME input and browser content.

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

## `title <text>` — setting a pane's OSC title (added 2026-08-14)

`title ORCHTITLE` clicks nothing and assumes nothing: it types `printf '\033]0;<text>\007'`
into the focused pane's shell and presses Return, which is the input path a real user has.
Click a terminal pane first so the shell has focus.

**Use it rather than writing the escape sequence inline.** The whole action block reaches
`eval`, and an OSC sequence carries a bare `;` — so `type printf '\033]0;X\007'` parses as two
commands, types half a sequence, and leaves the title untouched while looking like the app
ignored it. Four `F-CORE-ACT` rows (02, 06, 07, 11) sat unexercised on exactly that mistake.

Verified live: `reference/linux-progress/title-gesture-test/03-after-title.png` shows the
`printf` typed into the pane and executed (prompt returned, 2 ms).

### Trap: `panel.list`'s `agent` and `title` fields are NOT activity observables

`panel.list` returns `{"agent":"","tab":"Terminal","title":"Terminal"}`. Neither field moves
when activity state changes, so **a null result from them means nothing at all**:

- `title` is the *tab label*. `tiller_terminal/src/lib.rs:134` says so outright — the context
  menu's `SetTitle` is a user-set label, deliberately distinct from `OscTitle`. It does not
  change when an OSC 0 title arrives, and that is by design.
- `agent` stays `""` even after a **successful** `notify`. Confirmed with a positive control:
  `ctl notify session=pane-1 status=running` returned `{"queued":"true"}` and the very next
  `panel.list` still read `agent:""`.

That control is the whole point. Reading `agent:""` after an OSC title looks exactly like
"Layer B is broken" — and it is *not evidence of that*, because the field reads empty when
Layer A demonstrably worked. A verdict filed off this instrument would have been a fabricated
defect. If you need to observe activity state, read it from the frame (tab dot, sidebar
status), not from `panel.list`.

Valid `notify` statuses, from `main.rs:1483`: `running`, `needs-input`/`needs_input`,
`done`/`finished`, `error`/`failed`. Anything else returns "notify has an unknown status" —
`working` is **not** one of them.

## New pointer and keyboard primitives — right-click, drag, scroll, chords (added 2026-08-15, P124)

The vocabulary was `ctl / click / move / type / key / title / shot`. It is now also
`rightclick / down / up / drag / scroll / chord / modclick`, all built the same way `title` was:
extending the persistent virtual-pointer and virtual-keyboard clients that `wayland-drive.sh`
starts **before** Tiller connects (trap 3), never a one-shot process racing the first bind.
`wayland-virtual-pointer.c` gained `rightclick`/`down`/`up`/`scroll` operations alongside its
existing `move`/`click`; the keyboard side reuses plain `wtype`, which was already proven safe as
a one-shot call once the persistent keeper has caused GPUI to bind `wl_keyboard` at startup (P112).
Every action below still runs `verify_nested_sway` and fails clearly (`FAIL: …`) when its
prerequisite tool or device is missing; `start_virtual_pointer`/`start_virtual_keyboard`'s
startup-condition `grep` guards were extended, not replaced, to cover the new verbs.

### `rightclick <x> <y>` — opens context menus

Same persistent device as `click`/`move`, sending `BTN_RIGHT` (`0x111`) instead of `BTN_LEFT`.

**Positive control:** right-clicked a live terminal pane (real PTY, `project.add` → click to focus
→ `rightclick`) and got the full 12-item context menu — Copy, Paste, Copy Context, Set Title, Copy
Pane ID, Copy Terminal ID, Split Left/Right/Above/Down, Clear Terminal, Close Terminal — not a
screenshot that merely contains the pane.
`reference/linux-progress/p124-wayland-primitives/p124-rightclick-context-menu.png`.

### `chord <mod> <key>` — modifier-held named key

One `wtype -M <mod> -k <key> -m <mod>` call. `<mod>` is anything `man wtype` accepts: `shift`,
`ctrl`, `alt`, `logo`, `altgr`, `capslock`.

**Positive control:** `chord shift F10` on a focused terminal pane opened the same context menu as
`rightclick`, live — the exact keyboard path `F-CORE-TERM-02`'s unit test exercises
(`shift_f10_opens_the_context_menu_from_the_keyboard`), now driven through real synthetic input
instead of only a `gpui::test` harness.
`reference/linux-progress/p124-wayland-primitives/p124-chord-shift-f10-menu.png`.

### `down <x> <y>` / `up <x> <y>` / `drag <x1> <y1> <x2> <y2> [steps]` — button-held drag

`down` presses `BTN_LEFT` with no release; `up` releases with no press; `drag` composes
`down` → `steps` (default 4) intermediate `move` waypoints → `up`, so the target sees real
intermediate motion, not a teleport many drop targets ignore.

**Positive control:** right-click → Split Right on a terminal to create a pane divider, then
`drag`ged the divider roughly 40% of the pane's width to the right. The divider's on-screen
position moved from the pane midpoint to clearly past it in the forced-repaint capture — compare
`p124-drag-divider-before.png` (divider at the split ratio's default midpoint) against
`p124-drag-divider-after.png` (divider well to the right) in
`reference/linux-progress/p124-wayland-primitives/`. This is a real GPUI `on_drag`/`on_drag_move`
target (`DraggedPaneDivider`, `tiller/src/main.rs`), not a synthetic stand-in.

### `scroll <x> <y> <steps>` — wheel/axis

Moves to `(x, y)`, then sends `<steps>` wheel notches as a real `zwlr_virtual_pointer_v1`
`axis`/`axis_source`/`axis_discrete` event sequence (15 libinput units per notch, matching a
physical wheel) — not a synthesized `PageUp`/`PageDown` keypress standing in for a wheel.
`<steps>` may be negative for the opposite direction.

**Positive control:** scrolled the Files panel's file list. `steps=8` moved the visible top row
from `.claude` down to `.tiller` (later entries, including `README.md`, scrolled into view at the
bottom); `steps=-8` immediately after restored the original top row exactly. Compare
`p124-scroll-files-before.png` and `p124-scroll-files-after.png` in
`reference/linux-progress/p124-wayland-primitives/`. Sign convention: positive scrolls the content
up (later items appear), matching a standard downward wheel motion.

### `modclick <mod> <x> <y>` — modifier-held click

`wtype -M <mod> -s 400 -m <mod>` backgrounded (wtype releases a modifier only when its own process
exits, so the hold must outlive the click), a 50 ms settle for the press to land, then a normal
`click`, then `wait` for the release. Guarded in **both** startup conditions since it needs the
pointer device and the keyboard device pre-created.

**Not independently proven end-to-end against a live app target** — flagged honestly rather than
claimed. `chord` and `click`/`rightclick`/`drag`/`scroll` are each proven above; `modclick` is a
straightforward composition of exactly those two already-proven mechanisms (modifier-hold via
`wtype`, click via the same persistent pointer FIFO) and there is no reason specific to Wayland
input delivery for it to behave differently. But every in-app target gated on
`event.modifiers.platform` on a mouse event turned out unusable for a *live* proof in the time
available:

- The terminal's platform-modifier-click-opens-a-link path (`F-TERM-UI-02`,
  `tiller_terminal/src/lib.rs:1071`) computes the clicked grid row/column directly from
  `event.position` without subtracting the terminal element's own `bounds.origin` — the paint path
  a few lines away (`bounds.origin.y + LINE_HEIGHT * line`) does add it, so the two are asymmetric.
  In the isolated `gpui::test` window (origin ≈ 0,0) this coincidentally hits the right cell; in
  the real multi-pane app the terminal is never at the window origin, and an exhaustive coordinate
  sweep (both axes, values spanning the entire plausible range) never triggered the link's
  `xdg-open` spawn. This looks like a genuine, pre-existing app defect independent of our lane —
  noted here for whoever next touches `F-TERM-UI-02`, not filed separately since it is outside this
  task's three assigned rows.
- `file_view.rs`'s own platform-click-opens-a-markdown-link path (`open_markdown_link`,
  `F-CORE-FILE-04`) emits `FileViewEvent::OpenFile`, but `Workspace::add_file_tab`
  (`tiller/src/main.rs:4499`) never subscribes to that event on the `FileView` it constructs — only
  a test harness does. Dead code in production.
- Chat transcript links only parse as clickable markdown from **assistant**-authored messages;
  `surface.chat.compose`/`.send`-posted user text renders as literal, unlinked text
  (`[Example](url)` shown verbatim), so this path needs a live agent turn to reach, which is out of
  this task's control.

If a future pass needs `modclick` proven against a live target, `F-TERM-UI-02`'s row/column bug
above is worth fixing first — it is currently the only reachable modifier+click target in the app.

## The Clone/Create project popover — a real layout defect, not a lane gap (P123, 2026-08-15)

`F-PRJ-06`/`07`/`09` were recorded `half-proven` against an "anchored-popover input-delivery gap."
**It is a genuine app defect, not a Wayland-lane limitation**: `Sidebar::render_project_form`
(`tiller_ui/src/sidebar.rs:1965`) builds a `.absolute().left(0).right(0).top(0).bottom(0)` overlay
styled like a full-window modal (dark scrim, centered card), but its nearest `.relative()` ancestor
is the Sidebar's own root div — `w(px(SIDEBAR_WIDTH))`, roughly 280px, not the window. The popover
therefore renders squeezed into the sidebar's own narrow column, flush left, not centered on the
window as its styling implies.

Once clicks are targeted at the popover's *actual* (sidebar-confined) position instead of the
window-center position its styling suggests, buttons register normally: Cancel closed the popover
on the first click in two separate drives (including one where the URL field was clicked/focused
immediately beforehand, matching the originally-reported sequence exactly), and the "Clone
repository" submit button — after one retry click — drove a real `git clone` subprocess whose
genuine stderr (`git: 'remote-tps' is not a git command`) rendered in the popover, proving the full
click → handler → subprocess → error-surfaced round trip completes. Full evidence, screenshots and
the discriminators run: `docs/linux-rewrite/tasks/P123-popover-click-routing.md`. Not fixed here —
this task owns no Rust.

## The embedded browser cannot be exercised on this lane at all

`wry`'s `build_as_child` accepts only an X11 (Xlib/XCB) parent on Linux. GPUI on this lane hands it a
Wayland surface, so `BrowserSurface::new` fails both its direct and adapter branches and the app's own
banner reads:

> Direct XCB build failed: the window handle kind is not supported; XCB→Xlib adapter failed: GPUI
> returned unsupported handle: `Wayland(WaylandWindowHandle { ... })`

Every later call then reports **`Browser child is unavailable`**. That string means *no webview was
ever constructed*; it is not evidence about the browser feature, the network, or the page.

**For browser rows the X11 lane is not a fallback — it is the only lane.** `Scripts/linux-drive.sh`
launches with `env -u WAYLAND_DISPLAY DISPLAY=:1`, and both halves matter: **`DISPLAY=:1` alone is not
enough**, because GPUI prefers Wayland whenever `WAYLAND_DISPLAY` is set. A hand-rolled launch that
exports only `DISPLAY` silently takes the Wayland path and reproduces the failure while looking like
an X11 run — this produced several false negatives before it was spotted. (Note the mirror image of
the rule already documented above: this lane must have `DISPLAY` *unset*, not empty. Each lane has to
clear the other's variable.)

Two failure modes look identical to a broken browser and are not:

- **No workspace selected.** `project.add` does **not** select a workspace. Without
  `workspace.select workspace=<id>` (the param is `workspace`, **not** `path`), browser calls return
  `no current workspace` or `no browser surface`.
- **A truncated wait.** `CONTROL_ACTION_TIMEOUT` (`main.rs:196`) is a hardcoded 5 s bound on control
  dispatch, so a `browser.wait timeoutMs=` above ~5 s fails with `control action timed out` — which
  reads as "the condition never happened". See `tasks/P126-…`.

Working recipe, proven end to end (`browser.get` → `loading:false`, `title:"Example Domain"`;
`browser.eval script=document.title` → `"Example Domain"`):

```bash
export TILLER_SOCKET=/tmp/<label>.sock TILLER_DB=/tmp/<label>.sqlite
Scripts/linux-drive.sh out.png '
  python3 Scripts/control-probe.py "$TILLER_SOCKET" project.add path=<repo>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" workspace.select workspace=<id>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.open url=<url>
  sleep 5
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.get
'
```

`Scripts/control-probe.py <socket> <method> [k=v ...]` sends one line-delimited control request and
prints the reply — `linux-drive.sh` has no `ctl` action of its own.

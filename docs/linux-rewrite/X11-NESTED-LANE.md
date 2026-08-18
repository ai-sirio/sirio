# The X11-nested lane — a private Xwayland, isolated from the user's own desktop

Built 2026-08-18 to settle the question `STATE.md`'s "one bucket this machine cannot re-exercise"
raised: eight `F-BRW` rows were `PASSED` on a Raspberry Pi's `DISPLAY=:1`, this box's `DISPLAY=:1`
is the **user's own COSMIC desktop** (not a dedicated X server), and `WAYLAND-LANE.md` already
proved the native-Wayland lane cannot render the embedded browser's page content — only its chrome.

**It works.** `Scripts/x11-nested-drive.sh` boots a private, disposable Xwayland with real DRI3 on
the real AMD GPU, launches Tiller against it, and the browser's actual page content renders —
verified with a local `file`-served page and with a live `https://example.com` fetch. No pointer or
keystroke reaches the user's session; the drive lock (`linux-drive.sh`'s reason to exist) is not
needed because this display belongs to nobody else.

## The mechanism, in one sentence

A nested `sway` on the wlroots **headless** backend — the same trick `wayland-drive.sh` already
uses for its own private Wayland display — is launched with `xwayland enable` **instead of**
`xwayland disable`, and *that* nested compositor's own Xwayland is what Tiller gets pointed at.
The private Wayland display underneath is incidental; the private **X11** display, `:N`, is the
point.

```
user's COSMIC session (wayland-1, DISPLAY=:1)
  └─ nested sway (WLR_BACKENDS=headless, real GLES2 renderer on /dev/dri/renderD128)
       └─ its own Xwayland, xwayland enable  ──►  DISPLAY=:N, nobody else's
            └─ Tiller, launched with DISPLAY=:N, WAYLAND_DISPLAY unset
```

## Recipe

```bash
cat > /tmp/<you>-sway.conf <<'EOF'
xwayland enable
default_border none
default_floating_border none
gaps inner 0
gaps outer 0
output HEADLESS-1 resolution 1280x800
EOF

env -u WAYLAND_DISPLAY -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/1000 \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    SWAYSOCK=/tmp/<you>-sway.sock \
    sway -d -c /tmp/<you>-sway.conf > /tmp/<you>-sway.log 2>&1 &
```

Then launch Tiller with `DISPLAY` **set** (opposite of the Wayland lane) and `WAYLAND_DISPLAY`
**unset**:

```bash
env -u WAYLAND_DISPLAY DISPLAY=:N \
    XDG_RUNTIME_DIR=/run/user/1000 \
    TILLER_DB=/tmp/<you>.sqlite TILLER_SOCKET=/tmp/<you>.sock \
    rust/target/debug/tiller > /tmp/<you>.log 2>&1 &
```

Capture with ImageMagick's `import`, not `grim` (`grim` speaks Wayland, this window is X11):

```bash
WINID="$(DISPLAY=:N xdotool search --onlyvisible --pid "$APP_PID" | head -1)"
DISPLAY=:N import -window "$WINID" /path/to/shot.png
```

Drive it with `xdotool` — real `XTestFakeButtonEvent`/`XTestFakeKeyEvent`, not a synthetic paste:

```bash
DISPLAY=:N xdotool mousemove --window "$WINID" 584 89 click 1
DISPLAY=:N xdotool key --window "$WINID" ctrl+a
DISPLAY=:N xdotool type --window "$WINID" --delay 20 -- 'http://127.0.0.1:8971/probe.html'
DISPLAY=:N xdotool key --window "$WINID" Return
```

**Use `Scripts/x11-nested-drive.sh` — it does all of this, plus cleanup that only ever touches its
own instance.** See its header comment for the full action vocabulary (`ctl`, `click`, `move`,
`rightclick`, `down`/`up`/`drag`, `scroll`, `type`, `key`, `chord`, `shot`).

```bash
Scripts/x11-nested-drive.sh /tmp/my-x11-shots '
  ctl project.add path=/abs/path/to/tiller-linux
  ctl browser.open url=http://127.0.0.1:8971/probe.html
  ctl tab.select index=3
  shot page-content
' 10
```

## The traps

### 1. `WLR_RENDERER=pixman` kills DRI3 stone dead — and the failure is silent until Vulkan init

`wayland-drive.sh` deliberately forces `WLR_RENDERER=pixman`: its app is a native Wayland client,
never asks the compositor for an X11-backed GPU pixmap, and pixman is the cheaper, simpler choice
for that lane. Copying that env var into an Xwayland-enabled sway config produces:

```
libEGL warning: DRI3 error: Could not get DRI3 device
libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering
WARNING: radv is not a conformant Vulkan implementation, testing use only.
vulkan: No DRI3 support detected - required for presentation
Note: you can probably enable DRI3 in your Xorg config
```

This reads exactly like "X11 still doesn't work here" — the same dead end `ENVIRONMENT.md` already
recorded for Xvfb/Xephyr, which really *do* lack any DRI3 path. It is not the same failure. Xvfb
and Xephyr have no GPU-backed allocator at all, ever. Nested Xwayland has one — the **compositor's**
renderer — and pixman is what took it away. **Leave `WLR_RENDERER` unset.** wlroots then
auto-selects GLES2 against the real DRM render node:

```
[wlr] [render/gles2/renderer.c:842] Creating GLES2 renderer
[wlr] [render/gles2/renderer.c:845] GL renderer: AMD Radeon Graphics (radeonsi, gfx1200, ...)
```

— and DRI3 is then genuinely present. `vulkaninfo --summary` under `DISPLAY=:N` lists both `radv`
(`AMD Radeon Graphics (RADV GFX1200)`) and `llvmpipe` as available devices once this is right.

### 2. Diffing `/tmp/.X11-unix` for the new socket has to snapshot BEFORE sway launches

sway's Xwayland starts **lazily** (`Initializing Xwayland (lazy=1)` at debug level) — the actual
X server process forks only once a real client connects — but it reserves and binds its
`/tmp/.X11-unix/XN` lock socket essentially immediately, well under 100 ms after the compositor
comes up, independent of any client. A snapshot-then-diff approach to learning `N` is race-free
**only if the "before" snapshot is taken before `sway` is even started.** The first version of
this lane's script took "before" after confirming the compositor was up (i.e. after the socket
had *already* appeared), so the diff never saw anything new and failed 100% of the time with
`no new Xwayland socket appeared` even though Xwayland was working correctly. Waiting for the
"Starting Xwayland on :N" **log** line instead is not a fix either — that line is written only
once a client actually connects, which can be several seconds late, and a script that blocks on
it before launching Tiller (the first client) is waiting on an event Tiller's own connection
attempt is what triggers.

### 3. `import -window root` fails; capture the app's own window id

`import -window root` on this nested Xwayland raises `unable to read X window image 'root':
Risorsa temporaneamente non disponibile` (EAGAIN) — plausibly because the root window here isn't
backed by a readable pixmap the way a real X server's is. Capturing the **application's own**
top-level window works cleanly. Finding it needs a filter, not just "the active window": a GPUI
X11 client owns several 1×1 helper windows (selection/DnD plumbing) alongside its real toplevel,
so `xdotool search --pid $APP_PID` alone returns three or four ids. Adding `--onlyvisible` filters
down to exactly the one that matters — confirmed by reading back its geometry
(`getwindowgeometry`), which showed `1280x800` against the helpers' effective invisibility.

### 4. `xdotool`'s absolute coordinates are window-relative, not what you'd guess from memory

A click at `(300, 88)` aimed at the browser's address bar landed on the **sidebar's project
filter** instead — the coordinate was correct for *some* widget, just not the one intended, and
the frame still changed (a visible focus ring appeared on the Filter field), which is exactly the
kind of result that can be misread as "the gesture worked" when it actually proves only that
*a* gesture landed *somewhere*. Read the coordinates off the current capture, not off a mental
model of the layout, before trusting a click's target.

### 5. `browser.open`/`browser.navigate` mis-normalize `file://` URLs — an app bug, not a lane bug

```
ctl browser.open url=file:///tmp/x11lane-test/probe.html
  -> {"surface":"surface:2","title":"","url":"https://file:///tmp/x11lane-test/probe.html"}
     (chrome then shows: Error resolving "file": Name or service not known)

ctl browser.navigate surface=surface:2 url=file:///tmp/x11lane-test/probe.html
  -> {"error":"browser.navigate failed: Could not resolve file:: invalid port value"}
```

Both control methods appear to prepend `https://` to any URL that doesn't already look like one
by their own heuristic, and that heuristic does not recognize `file:` as a scheme it already has.
`http://` and `https://` URLs are unaffected — verified with both a local
`python3 -m http.server` page and a live fetch of `https://example.com`, both of which rendered
correctly. **Route `file://` proof-of-render tests through a throwaway local HTTP server instead
of a bare `file://` URL** until this is fixed upstream in `browser.rs`'s URL normalization; it is
a pre-existing defect this lane exposed, not something introduced by it, and it reproduces
identically on `DISPLAY=:1`.

## What it proves

Captured 2026-08-18 with `Scripts/x11-nested-drive.sh`, `TILLER_X11_LABEL=finalcheck`:

- `01-baseline.png` — full app chrome (sidebar, tab bar, Files panel), `6175` colours.
- `02-final-proof.png` — the Browser tab, address bar reading
  `http://127.0.0.1:8972/probe.html`, and the content area showing a solid `#ff2d95` field with
  96 px white text reading `X11 LANE PROOF 42` — the actual page, not the chrome around it —
  `8944` colours.

A second, independent run drove the same navigation with `https://example.com` and captured its
real rendered text ("Example Domain", the actual paragraph copy, the "Learn more" link) — matching
`reference/linux-progress/p86-post-browser.png`, the last time this project had page content on
X11 at all. A third run drove the navigation with **real `click`/`key`/`type` gestures** (address
bar click, `Ctrl+A`, typed URL, `Return`) rather than the control socket, landing on the identical
page — closing both the "state" and "gesture" halves `WAYLAND-LANE.md` insists a row must clear
separately.

**Every `F-BRW` row can be re-driven here**, on this machine, without touching the user's desktop
and without a drive lock. `WAYLAND-LANE.md`'s "Route every `F-BRW` row to the X lane" now has a
lane on this box that isn't someone's real login session.

## What's still open

- `chord`/named-key coverage beyond `ctrl+a`/`Return` is implemented (xdotool's native
  `mod+mod+key` syntax) but not individually re-verified per combination the way
  `WAYLAND-LANE.md`'s pointer/keyboard primitives were (P112/P124). Treat `chord` as available,
  not yet as separately proven per-modifier.
- Non-ASCII text and IME input are untested here, same caveat `WAYLAND-LANE.md` carries for its
  own lane.
- `browser.open`/`browser.navigate`'s `file://` normalization bug (trap 5) should be filed and
  fixed independently of this lane; it blocks the cheapest possible positive control
  (`file://` with no server) for every future critic until then.

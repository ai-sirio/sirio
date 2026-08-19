# Critic report — `feat(display): force X11 on Linux` (7d4d2182)

Adversarial live-drive critic. Built the binary myself, booted my own nested compositors, and
launched the app with my own `env` invocations rather than the standard harness — the standard
harness (`Scripts/x11-nested-drive.sh`) strips `WAYLAND_DISPLAY` before exec and therefore cannot
exercise the central case this commit exists for. Every claim below is something I personally
observed with a named instrument; where I could not exercise something, that is stated.

## Setup

```
git rev-parse HEAD          # 7d4d2182f9583f1ef324e8fd95566221366d841c (already checked out)
cd rust && CARGO_TARGET_DIR=/var/tmp/tt-x11-crit cargo build -p tiller
```

Binary: `/var/tmp/tt-x11-crit/debug/tiller` (built fresh from 7d4d2182, not reused from any other
critic's target dir). A disposable local git fixture was used for `project.add` in every case
(`/tmp/.../scratchpad/x11f-critic/fixture-repo`, one commit, never `tiller-linux` itself), and a
local HTTP server at `127.0.0.1:8977` served a fixture page (avoids the pre-existing `file://`
URL-normalization bug X11-NESTED-LANE.md already documents).

**Never touched `DISPLAY=:1`/`wayland-0`/`wayland-1`.** All compositors below are private, nested
`sway` instances on the wlroots headless backend, booted the same way
`Scripts/x11-nested-drive.sh`/`Scripts/wayland-drive.sh` do (`WLR_BACKENDS=headless`,
`XDG_RUNTIME_DIR=/run/user/$(id -u)`, `SWAYSOCK` pinned per-instance). `Scripts/x11-nested-drive.sh`
itself was run unmodified for cases 1 and 5 only; cases 2–4 used my own launch commands (in
`/tmp/.../scratchpad/x11f-critic/`) because the shipped harness's `env -u WAYLAND_DISPLAY` is
exactly the workaround this commit replaces, and using it would prove nothing about the new code.

**A trap I fell into and want on record**: my first instinct was to verify the environment
mutation via `tr '\0' '\n' < /proc/<PID>/environ`. That file is a **snapshot taken at `execve()`**
and is never updated by a process's own later `setenv`/`unsetenv` — which is exactly what
`std::env::set_var`/`remove_var` are. So `/proc/PID/environ` showing `WAYLAND_DISPLAY` still
present after `prepare_environment()` ran is **expected and not a bug**; it proves nothing either
way about whether the mutation happened. I dropped it as primary evidence and used `xdotool search
--pid` (does the process own an X11 window at all), a `swaymsg get_tree` cross-check keyed on `pid`
(does it own a Wayland window instead), and the app's own stderr line — all of which reflect live
process state, not exec-time state.

**A second trap, self-inflicted**: my first pass at cases 2–4 checked window state immediately
after the control socket appeared, with no wait. That raced GPUI's window creation — one run
showed `NO WINDOWS FOUND` and a blank 4.8 KB screenshot for case 3, which briefly looked like a
genuine headless/windowless failure. Waiting up to 15s and re-checking on a `pid`-keyed
`swaymsg get_tree` query showed a fully-rendered window every time. All results below are from the
waited, re-verified runs — the timestamps in the case-3 section show both the false negative and
its correction so the mistake is traceable rather than quietly dropped.

---

## 1. POSITIVE CONTROL

```
TILLER_X11_LABEL=x11fcrit15 TILLER_X11_BIN=/var/tmp/tt-x11-crit/debug/tiller \
  Scripts/x11-nested-drive.sh <outdir> '
    ctl project.add path=<fixture-repo>
    shot baseline
  ' 8
```

Exit 0. `01-baseline.png` (before `project.add`, 4562 colours) and `02-baseline.png` (after,
6569 colours) both show a fully painted app: sidebar, tab bar, Chat/Terminal panes, Files panel —
see `case1-positive-control-empty.png`. The app starts and renders. **PASS.**

## 2. THE CENTRAL CASE — Wayland session with XWayland, `WAYLAND_DISPLAY` still set at launch

Exact commands (private nested sway, `xwayland enable`, then the app launched with **both**
`WAYLAND_DISPLAY` and `DISPLAY` set — nothing stripped):

```bash
env -u WAYLAND_DISPLAY -u DISPLAY XDG_RUNTIME_DIR=/run/user/$(id -u) \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 SWAYSOCK=/tmp/x11fcrit2-sway.sock \
    sway -d -c /tmp/x11fcrit2-sway.conf   # xwayland enable; -> WD=wayland-2, DISPLAY_N=:3

env WAYLAND_DISPLAY=wayland-2 DISPLAY=:3 \
    XDG_RUNTIME_DIR=/run/user/$(id -u) \
    TILLER_DB=/tmp/x11fcrit2.sqlite TILLER_SOCKET=/tmp/x11fcrit2.sock \
    /var/tmp/tt-x11-crit/debug/tiller
```

Full transcript: `/tmp/.../scratchpad/x11f-critic/case2_central.sh` + `case2.out` (script), plus
interactive follow-up once the app was confirmed alive.

- **`/proc/$PID/environ`** (informational only, per the trap above — NOT proof of the mutation):
  `WAYLAND_DISPLAY=wayland-2` and `DISPLAY=:3`, i.e. the *pre-exec* values. Expected, not a fail.
- **stderr**: `[display] Wayland session detected; running on X11 (XWayland) so the embedded
  browser can attach. Set TILLER_FORCE_X11=0 to stay Wayland-native without it.` — present.
- **`xdotool search --onlyvisible --pid $PID` on `DISPLAY=:3`** (retry loop, up to 15s): found
  `WINID=4194305`, geometry `1470x833` at a real offset — a genuine X11 top-level owned by this
  PID.
- **Cross-check, `swaymsg -t get_tree` on the Wayland side** (`WD=wayland-2`), filtered for any
  node whose `app_id`/`window_properties.class` is set: `NO WAYLAND WINDOWS FOUND`. (This filter
  turned out to be too narrow in general — see case 3 below, where a real native-Wayland GPUI
  window has neither field set and only shows up as a `con` with a matching `pid`. For case 2 I
  did not re-run the `pid`-keyed version, so treat "no `app_id`/`class` node" as corroborating,
  not conclusive, evidence that it is *not* a Wayland client — the X11-side proof below is the
  decisive one.)
- **Browser tab, local fixture**: after `project.add` + `workspace.select` (browser.open requires
  a current workspace — `F-CTRL-BROWSER-02`; discovered live, not assumed), `ctl browser.open
  url=http://127.0.0.1:8977/probe.html` → `ok:true`. `ctl browser.wait` → `loading:false`,
  `title:"x11f-critic probe"`. `import -window 4194305` shows the actual green fixture page with
  its "X11F CRITIC PROBE 77" text, not chrome-only — `case2-central-local-fixture.png`.
- **Browser tab, remote URL**: `ctl browser.navigate url=https://example.com` → `ctl browser.wait`
  → `title:"Example Domain"`. Screenshot shows the real "Example Domain" heading, paragraph, and
  "Learn more" link — `case2-central-remote-example-com.png`.
- **stderr GTK/GDK check**: one critical, verbatim:
  ```
  (tiller:1926639): Gdk-CRITICAL **: 16:26:51.060: _gdk_frame_clock_freeze: assertion 'GDK_IS_FRAME_CLOCK (clock)' failed
  ```
  Fires once, right at startup. **Not present** in case 1/5's plain-X11 log or case 3's
  unforced-Wayland log (checked directly, see case 3 below) — this is specific to the runtime
  env-mutation path (clearing `WAYLAND_DISPLAY`/setting `GDK_BACKEND=x11` *after* the process has
  already started under a Wayland-session environment), not to running on X11 in general. It did
  not stop anything from working — both navigations above completed and rendered correctly — but
  it is a real, reproducible difference from a clean X11 boot and I'm reporting it verbatim rather
  than filing it as noise. Also present: `WARNING: radv is not a conformant Vulkan implementation,
  testing use only` (pre-existing, unrelated to this change) and a benign a11y-bus warning (no
  bus in this sandboxed environment).

**PASS.** This is the case the shipped harness cannot exercise, and it is fully proven live:
XWayland client, environment prepared correctly, browser renders both a local and a remote page.
One non-fatal `Gdk-CRITICAL` noted for the record.

## 3. THE FOOTGUN — no usable `DISPLAY`

Nested sway booted with `xwayland disable` (`WD=wayland-2`, no X11 socket at all). Three
sub-cases, same compositor, app relaunched fresh each time.

```bash
# unset
env -u DISPLAY WAYLAND_DISPLAY=wayland-2 XDG_RUNTIME_DIR=/run/user/$(id -u) \
    TILLER_DB=... TILLER_SOCKET=... /var/tmp/tt-x11-crit/debug/tiller
# empty
env DISPLAY="" WAYLAND_DISPLAY=wayland-2 ... same as above
# whitespace-only
env DISPLAY="   " WAYLAND_DISPLAY=wayland-2 ... same as above
```

**First pass (recorded, then corrected — see the trap noted above)**: checking `swaymsg get_tree`
and `grim` immediately after the control socket appeared showed `NO WINDOWS FOUND` and a flat
4789-byte screenshot (solid grey, no content) for all three sub-cases. That looked like exactly
the catastrophic failure mode item 3 warns about — headless/windowless — so I did not accept it
and re-ran with a wait.

**Corrected pass**, waiting up to 15s and querying `swaymsg get_tree` for a `con` node whose `pid`
field matches the app's PID (GPUI's native-Wayland windows set neither `app_id` nor
`window_properties.class`, which is why the first, class-filtered query found nothing even once
the window existed):

| sub-case | window rect found | `/proc/PID/environ` | stderr `[display]` line | screenshot |
|---|---|---|---|---|
| `DISPLAY` unset | `{x:0,y:0,w:1280,h:800}` | `WAYLAND_DISPLAY=wayland-2` only | `no usable DISPLAY, so X11 cannot be forced; staying on Wayland. The Browser tab cannot render a page here.` | 34661 B, full UI |
| `DISPLAY=""` | `{x:0,y:0,w:1280,h:800}` | `DISPLAY=`, `WAYLAND_DISPLAY=wayland-2` | same message | 32915 B, full UI |
| `DISPLAY="   "` | `{x:0,y:0,w:1280,h:800}` | `DISPLAY=   `, `WAYLAND_DISPLAY=wayland-2` | same message | 32915 B, full UI |

All three screenshots (`case3-footgun-display-unset.png`, `-empty.png`, `-whitespace.png`) show
the full, normal app chrome (sidebar, tab bar, Chat/Terminal, Files) — the same "no worktree
selected" starting screen as the positive control, not blank, not a crash, not headless. No GTK/GDK
critical in any of the three app logs (`grep -iE 'critical' <log>` → empty), confirming the
`Gdk-CRITICAL` in case 2/4 is tied to the forcing path, not to this app in general.

**PASS**, all three `DISPLAY` variants: the app stays a normal Wayland client with a visible
window, exactly as `NoXServer` promises, and does **not** go headless. I'm flagging the false
negative in my own first pass explicitly because it is the kind of result that, taken at face
value, would have produced a wrong NOT-CLEARED verdict on the single most safety-critical case in
this whole review.

## 4. THE OPT-OUT

Same compositor/launch shape as case 2 (`WAYLAND_DISPLAY=wayland-2`, `DISPLAY=:3`, XWayland
available), with `TILLER_FORCE_X11` varied.

### `TILLER_FORCE_X11=0`

```bash
env WAYLAND_DISPLAY=wayland-2 DISPLAY=:3 TILLER_FORCE_X11=0 \
    XDG_RUNTIME_DIR=/run/user/$(id -u) TILLER_DB=... TILLER_SOCKET=... \
    /var/tmp/tt-x11-crit/debug/tiller
```

- Waited window: `swaymsg get_tree` (pid-keyed) found `{x:0,y:0,w:1280,h:800}` — a real Wayland
  window.
- `xdotool search --pid` on `DISPLAY=:3`: **empty** — not an X11 client.
- stderr: `[display] TILLER_FORCE_X11 is off: staying Wayland-native. The Browser tab cannot
  render a page on Wayland.`
- `grim` screenshot: 34646 B, full normal UI (`case4-optout-off-baseline.png`).
- `project.add` + `workspace.select` + `ctl browser.open url=http://127.0.0.1:8977/probe.html` →
  `ok:true` (the tab/chrome is created), but `browser.wait`/`browser.get` afterward show
  `title:""`, `loading` stuck, `timedOut:true` — the page itself never loads. Screenshot
  (`case4-optout-off-graceful-failure.png`) shows the exact pre-existing failure banner
  `DECISION-browser-on-wayland.md` documents: *"Direct XCB build failed: the window handle kind is
  not supported; XCB→Xlib adapter failed: GPUI returned unsupported handle:
  Wayland(WaylandWindowHandle {...})"*. The app stayed alive and fully interactive throughout
  (confirmed with `kill -0 $PID` after the failed navigation, and the sidebar/chrome are still
  live in the screenshot).

**PASS**: Wayland client, real window, no crash, browser fails gracefully rather than taking the
app down — and it fails with the exact same message the pre-existing native-Wayland path produces,
which is the correct behaviour for "opt out and accept the old limitation."

### `TILLER_FORCE_X11=maybe` (must NOT opt out)

Same launch, `TILLER_FORCE_X11=maybe`.

- stderr: the **ForceX11** message (`Wayland session detected; running on X11 (XWayland)...`), not
  the opt-out message.
- `swaymsg get_tree` (pid-keyed, Wayland side): no window found for this pid.
- `xdotool search --pid` on `DISPLAY=:3`: `WINID=4194305`, a real X11 window.
- `import -window 4194305`: baseline screenshot matches case 2's (32594 B,
  `case4-optout-maybe-still-x11.png`).
- `project.add` + `workspace.select` + `browser.open` + `browser.wait` → `title:"x11f-critic
  probe"`, `loading:false` — the local fixture page actually rendered
  (`case4-optout-maybe-browser-renders.png`, matches case 2's local-fixture render pixel-for-pixel
  in content).
- Same `Gdk-CRITICAL` as case 2, same non-fatal outcome, verbatim:
  `(tiller:1977551): Gdk-CRITICAL **: 16:31:53.068: _gdk_frame_clock_freeze: assertion
  'GDK_IS_FRAME_CLOCK (clock)' failed` — reproduces the case-2 finding rather than being a one-off.

**PASS**: an unrecognised value is correctly *not* treated as an opt-out; X11 is forced and the
browser genuinely renders, exactly as the code's own test
`the_off_switch_uses_the_same_vocabulary_as_the_socket_flag` claims for `choose()` — and this is
the live confirmation that the same holds for `prepare_environment()`'s actual effect, not just
the pure function.

## 5. NO REGRESSION — plain X11, no `WAYLAND_DISPLAY` at all

This is exactly what `Scripts/x11-nested-drive.sh` already does (`env -u WAYLAND_DISPLAY
DISPLAY=$N`), so case 1's run above doubles as this case: `02-baseline.png`
(`case5-plain-x11-full-render.png`) shows the full app UI, 6569 colours, `project.add` succeeded,
no `[display]` line in the log at all (confirmed: `cat /tmp/x11fcrit15.log` shows only the
`[control] listening...` and the radv warning — the `ForceX11` branch's `eprintln!` is gated on
`on_wayland`, which is false here, so silence is the *correct* behaviour, not a missed case). No
`Gdk-CRITICAL`. **PASS**, unchanged from pre-existing behaviour.

## 6. Platform gating

```
grep -rn 'GDK_BACKEND\|"DISPLAY"\|"WAYLAND_DISPLAY"\|var_os("DISPLAY")\|var_os("WAYLAND' \
  rust/crates --include='*.rs' | grep -v 'display_backend.rs'
```
→ one hit, and it's prose inside `main.rs`'s doc-comment above the `mod` declaration, not code.
No other file in the tree reads or writes these three variables.

`mod display_backend;` is `#[cfg(target_os = "linux")]` (`main.rs:66-67`); the call site
`display_backend::prepare_environment();` in `fn main()` carries the same `#[cfg(target_os =
"linux")]` immediately above it (`main.rs:11778`). There is exactly one `main.rs` / one `main()` in
the `tiller` binary crate (`find rust/crates/tiller/src -iname '*main*'` → one file) — no separate
macOS/Windows entry point exists that would need a counterpart branch written into it.

**I agree no macOS/Windows counterpart is needed.** `DISPLAY`/`WAYLAND_DISPLAY`/`GDK_BACKEND` are
X11/Wayland/GTK-specific; macOS's Cocoa/AppKit and Windows' Win32 backends have no equivalent
env-driven backend selection for GPUI to read, so there is nothing analogous to prepare on those
platforms. **PASS.**

## 7. `cargo test -p tiller --bin tiller display_backend`

```
$ cd rust && CARGO_TARGET_DIR=/var/tmp/tt-x11-crit cargo test -p tiller --bin tiller display_backend
running 4 tests
test display_backend::tests::opting_out_wins_over_a_usable_display ... ok
test display_backend::tests::a_usable_display_is_forced_to_x11 ... ok
test display_backend::tests::without_a_display_nothing_is_forced_even_when_asked ... ok
test display_backend::tests::the_off_switch_uses_the_same_vocabulary_as_the_socket_flag ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 198 filtered out
```

**Trying to break them — and the gap they leave**: all 4 tests call `choose(display, force_flag)`
directly, the pure decision function. **None of them call `prepare_environment()`** — the function
that actually does the `unsafe { remove_var / set_var }` work and is the one thing that can be
wrong in a way `choose()` passing gives zero protection against. Concretely, these tests would
still pass green if `prepare_environment()`:
- set `GDK_BACKEND` to the wrong string (e.g. a typo, or `"x-11"`), or forgot to set it at all;
- cleared the wrong variable (e.g. `DISPLAY` instead of `WAYLAND_DISPLAY`);
- called `choose()` with its two arguments transposed;
- swallowed the `unsafe` block's effect entirely (e.g. behind a stray early return).

Every one of those bugs would be invisible to `cargo test` and would only surface as exactly the
symptom the commit message calls out — "a real X11 window the webview still cannot attach to... a
symptom identical to changing nothing" — which is precisely why case 2's live drive (verifying via
`xdotool`/`swaymsg`/actual browser rendering, not `/proc/environ`) is load-bearing here, not
optional. `prepare_environment()` is close to untestable in a normal unit test (it mutates real
process env and depends on being first-in-process), so this gap is structural rather than
sloppy — but it means the test suite alone is **weak evidence** for this commit; the live proof is
what actually clears it.

---

## Follow-up — F-WIN-06's actual VERIFY: the two key chords, driven for real

The seven items above are the display-backend commit's own review. Separately, the ledger flagged
that `F-WIN-06`'s VERIFY was never actually driven by anyone: *"Enable the universal workspace in
the test build, press `⇧⌘L`, then press `⌘L`, and confirm a browser tab and focused address field
respectively. SRC: `App/TillerApp.swift:76`."* Every browser tab in evidence up to this point —
including in the seven items above — was created from the `+` menu, and an earlier pass
(`FINISH-window-persist.md`) explicitly substituted the `+` menu for the chord and marked the row
PASSED on that basis. A menu click proves the *action* exists; it proves nothing about whether the
*chord* reaches it. This section drives the chords themselves, as real key events, against the
same warm build.

### 0. The "universal workspace" precondition

`App/TillerApp.swift:76` gates both commands behind `WorkspaceEngineGate.isEnabled`
(`App/Workspace/WorkspaceEngineGate.swift`) — a macOS-only flag (`UserDefaults` key
`workspace.universalEngine` / env override `TILLER_UNIVERSAL_WORKSPACE`) that **defaults to
`true`**. So on macOS these two menu commands are present out of the box; nothing needs enabling
in the ordinary case.

```
grep -rn 'WorkspaceEngineGate\|universal.workspace\|UniversalWorkspace' rust/crates -i
```
→ **zero hits.** This gate, and the concept of a "universal workspace" toggle, does not exist
anywhere in the Rust port — confirmed independently by an earlier critic pass (`F-AUTO-09` in
`fullapp/F-AUTO.md`) and reconfirmed here. This is not a reachability problem to work around: the
gate simply has no counterpart to enable, and its absence doesn't block anything by itself — the
Browser *tab* is already unconditionally available via the `+` menu in this port, gate or no gate.
The actual question is narrower and answerable directly: do the two *chords* reach anything?

### 1. Source: no keybinding exists for either chord, anywhere

```
grep -rn 'KeyBinding::new' rust/crates --include='*.rs'
```
Every binding in the whole tree (30+ call sites across `main.rs`, `panes.rs`, `tab_bar.rs`,
`chat.rs`, `settings.rs`) was enumerated by hand. None mention a browser action or an `l`/`L` key
combination of any kind. `linux_window_shortcuts()` (`main.rs:129-140`) — the single table this
codebase uses for every other macOS-chord-to-Linux-chord conversion (e.g. `⇧⌘O` →
`"ctrl-shift-o"`, commented at `main.rs:135-136` as the explicit Linux stand-in) — lists exactly
six commands, and neither "New Browser" nor "Focus Address Bar" is among them.

Also checked: no keymap asset file exists (`find rust -iname '*keymap*'` → nothing; no
`load_keymap`/`Keymap::` call anywhere) — every binding in this app is a literal `KeyBinding::new`
call in source, so the grep above is exhaustive, not a sample.

**The address-bar half is a stronger gap than "unbound."** There is no `FocusAddressBar` action —
or any action — for it at all (`grep -rn 'FocusAddressBar\|focus_address\|AddressBar'` →
zero hits). `rust/crates/tiller_ui/src/browser.rs:1429` gives the address field a `FocusHandle` and
focuses it from an `on_click` mouse handler — the widget itself is focusable, it simply has no
keyboard path to it, not even an inert one.

### 2. Live drive — New Browser chord

Build reused from the warm `/var/tmp/tt-x11-crit` target. Same private nested-Xwayland shape as
case 2 above (`TILLER_X11_LABEL=win06crit TILLER_X11_KEEP=1 Scripts/x11-nested-drive.sh`), then
driven directly against the kept-alive instance (`DISPLAY=:3`, `WINID=4194305`,
`SWAYSOCK=/tmp/win06crit-sway.sock`) with hand-written `xdotool key --window $WINID <chord>` calls
— real `XTestFakeKeyEvent`s, not the control socket. `project.add` + `workspace.select` against a
disposable fixture repo first, then a real click on the Chat pane to give the **window** X11 input
focus before any chord (`winb06-00-baseline-focused.png`, 7160 colours).

Tried the Linux spelling this codebase's own `linux_window_shortcuts()` table uses everywhere else
(cmd→ctrl, shift→shift), plus the literal macOS spelling in case GPUI maps `cmd` to `super` on
Linux:

| chord tried | exact command | `browser.get` after | screenshot | colours |
|---|---|---|---|---|
| `ctrl+shift+l` | `xdotool key --window 4194305 ctrl+shift+l` | `no browser surface` | `winb06-01-after-ctrl-shift-l.png` | 7160 |
| `ctrl+l` | `xdotool key --window 4194305 ctrl+l` | `no browser surface` | `winb06-02-after-ctrl-l.png` | 7160 |
| `super+shift+l` | `xdotool key --window 4194305 super+shift+l` | `no browser surface` | `winb06-03-after-super-shift-l.png` | 7160 |
| `super+l` | `xdotool key --window 4194305 super+l` | `no browser surface` | `winb06-04-after-super-l.png` | 7160 |

All four screenshots are colour-identical to the baseline (7160) — not just "no browser tab
appeared," literally **zero pixels changed** for any of the four chords. `browser.get` (the same
control-socket probe used throughout this report, here used only as a read-only assertion, not to
drive the action) returned `"no browser surface"` after every one. **FAIL** — no candidate spelling
of the new-browser chord does anything.

### 3. Live drive — Focus Address Bar chord

Opened a real Browser tab the only way this build supports — `+` menu → New Browser
(`winb06-05-plus-menu-open.png` shows the real menu, coordinates read off this screenshot rather
than guessed, per the harness's own documented coordinate trap). It auto-loaded
`https://example.com/` (`winb06-06-browser-open-via-menu.png`; `browser.get` confirms
`title:"Example Domain"`).

Deliberately defocused the address field — clicked into the page content area, not the address bar
(`click 600 400`, `winb06-07-defocused-address-bar.png`) — then:

```
xdotool key --window 4194305 ctrl+l
xdotool type --window 4194305 --delay 20 -- CHORDTEST123
```

Result: `winb06-08-after-ctrl-l-and-type-no-change.png` is byte-identical in size to the
pre-chord screenshot (62727 B both) and visually identical — address bar still reads
`https://example.com/`, no cursor, no marker text anywhere on screen. The chord did not focus the
field and the typed characters landed nowhere observable.

**Control, to validate the instrument itself**: clicked the address bar directly
(`click 584 89`), `ctrl+a`, typed the identical marker:

```
xdotool key --window 4194305 ctrl+a
xdotool type --window 4194305 --delay 20 -- CHORDTEST123
```

`winb06-09-direct-click-proves-widget-works.png` shows the address field genuinely replaced with
`CHORDTEST123` and a visible text cursor. This confirms the marker-typing method is valid and the
widget itself works correctly — the `ctrl+l` chord specifically is what does nothing, not the
field or the typing technique.

**FAIL** — the address-bar chord does not focus the field, under any candidate spelling tried
(and per item 1, there is no action registered for it to reach even in principle).

### Verdict on F-WIN-06's VERIFY

Both halves **FAIL** when driven as the row actually specifies — as key chords, not as menu
clicks. This is not an unreachable-precondition situation (item 0 rules that out: the "universal
workspace" gate has no counterpart to block on, and the Browser tab is unconditionally available
regardless). It is a straightforward gap: the macOS build's `⇧⌘L`/`⌘L` menu commands were never
given a Linux keybinding, and `⌘L`'s underlying action (focus the address field) was never even
built as an action — only as a mouse-click handler. The tab-creation *capability* exists and works
(via the `+` menu, and via the control socket, both previously proven); the *chords* the row asks
about do not exist. Recommend recording this precisely rather than as "PASSED (substitute route)"
or "unreachable" — neither describes what was actually found.

## Summary

**7d4d2182 review (items 1–7)** — verdict **CLEARED**:

| # | Case | Verdict |
|---|---|---|
| 1 | Positive control | PASS |
| 2 | Central case — Wayland+XWayland, `WAYLAND_DISPLAY` set at launch | PASS |
| 3 | Footgun — no usable `DISPLAY` (unset/empty/whitespace) | PASS |
| 4 | Opt-out (`=0`) and non-opt-out (`=maybe`) | PASS |
| 5 | No regression, plain X11 | PASS |
| 6 | Platform gating | PASS |
| 7 | Unit tests | PASS, but structurally weak — see above |

**Follow-up — F-WIN-06's own VERIFY (new-browser and address-bar chords)** — verdict **FAIL**, both
halves, per the "Follow-up" section above. This is a separate finding from the 7d4d2182 review: the
display-backend commit is what makes the Browser tab's *content* render at all (proven above), but
the specific keyboard chords `F-WIN-06`'s VERIFY names were never wired to anything in this port,
independent of that commit.

Non-blocking findings for the record:
- A reproducible `Gdk-CRITICAL: _gdk_frame_clock_freeze: assertion 'GDK_IS_FRAME_CLOCK (clock)'
  failed` fires once at startup specifically when X11 is forced at runtime from a
  `WAYLAND_DISPLAY`-set process (cases 2 and 4-`maybe`), absent when X11 is the environment from
  exec time (cases 1/5) or when nothing is forced (case 3). Did not stop anything from working in
  either occurrence.
- `display_backend`'s own unit tests only cover the pure `choose()` function, not
  `prepare_environment()`'s actual env mutation — noted under item 7.

**CLEARED** (7d4d2182, items 1–7). The F-WIN-06-VERIFY follow-up above is scored separately: both
key chords **FAIL** to reach anything, live-driven and source-confirmed.

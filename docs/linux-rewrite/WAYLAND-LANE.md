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

## 2026-08-17 — running this lane on the Pi 5, and five faults found by running it

The lane works unmodified on the Raspberry Pi 5 (see `ENVIRONMENT.md`'s top section for the box).
Everything below was found by *driving* it, not by reading it, and four of the five are now fixed
in `Scripts/wayland-drive.sh`. They are recorded because each one fails **silently** — every symptom
reads as "the app ignored me", which is the most expensive wrong answer this project keeps buying.

**The parent compositor is not optional and is not yours to kill.** Nothing renders on a
monitor-less box without it, and nested lanes are also called `sway`, so `pkill -x sway` kills the
parent *and* every other agent's lane. Once the parent died, a leaked nested compositor took the
freed `wayland-1` name, so `WAYLAND_DISPLAY=wayland-1` still resolved, drives still started, and
every frame came back black with a MESA `failed to choose pdev` line buried in the app log.

```bash
Scripts/pi-session.sh status     # exits 1 if the parent is down
Scripts/pi-session.sh start      # idempotent
eval "$(Scripts/pi-session.sh env)"
```

The parent now publishes the socket name it was given (`/tmp/tiller-parent.display`) instead of
being guessed at, and is identified by its config path. **Kill only your own instance**:
`pkill -x "$(basename "$TILLER_WL_BIN")"`.

| fault | symptom | status |
|---|---|---|
| `kill_ours` used `pgrep -x tiller` | Linux caps `comm` at 15 chars and `pgrep -x` matches `comm`, so a pinned snapshot named `/tmp/L2crit-tiller` matched **nothing** — pgrep only warns on stderr. Every pinned instance survived cleanup *and* `pkill -x tiller` | **fixed** — pattern truncated to 15 and taken from `$BIN`. *This row was wrong for a while and is worth reading as a caution:* only `cleanup()` got the `$BIN` treatment; the start-of-run pre-kill still passed the literal `tiller` until 2026-08-20, so the entry read "fixed" while half the fix was missing. Flagged by a builder who hit it, not by the table |
| `kill_ours` sent the signal and **did not wait** | `kill` requests an exit; this app has PTYs and a SQLite database to close and can take seconds, and the caller's next move is to boot a replacement — so nothing structurally prevented two instances sharing one `/tmp/$LABEL.sqlite`. That state would matter, because a departing instance still holds the **old** project catalog in memory and its final `write_catalog` upserts every field from that stale copy and deletes any project missing from its own `desired_ids`, which would silently revert or remove rows the new instance just wrote — and never as an error. **The harm is unreproduced, and the row should be read that way.** A builder reported hitting two live copies on one DB mid-drive (F-PER-07 pass, 2026-08-20). Trying to reproduce it: pre-fix script, renamed binary, `KEEP=1` relaunch under one label, sampling every 100 ms across the whole relaunch — **max 1 concurrent instance, zero samples with two**. Killing the *compositor* drops the app's Wayland connection and reaps it quickly enough that the window did not open. So the two code defects are real, the failure they could cause is not demonstrated here, and whether this explains that report's one unexplained observation — an empty read with the app still running — remains **a hypothesis, not a finding** | **fixed anyway, as hardening**: `kill_ours` now waits for each victim to actually exit (6 s, then `SIGKILL`), which is justified by the code defect alone and does not depend on the incident. Verified before/after with a renamed binary. Worst case remains `TILLER_WL_KEEP=1`, where `cleanup()` deliberately does not run and the start-of-run pre-kill is the only guard. If a row reads back empty or reverted *with the app up and the sidecars present*, count the instances before concluding anything about the app |
| the virtual keyboard expired after 10 min | a longer `TILLER_WL_KEEP` session lost **all** keyboard input silently; `type`/`key`/`chord` still exit 0 and nothing reaches the app. Already recorded once as a false negative | **fixed** — 4 h, `TILLER_WL_KEYBOARD_HOLD_MS` |
| `shot()` guessed the relayout with `sleep 1` | on a loaded Pi it captured a full-size frame of a window still laid out at the nudge size, black down two edges. Coordinates read off such a frame land in dead space | **fixed** — polls sway for the relayout, `TILLER_WL_REPAINT_SETTLE` |
| the `settle` argument's default of 6 | tuned on the x86 box; too short here. A near-blank first frame is settle, not the app | **use 10–30**, and read the frame's colour count |
| `title <text>` against an interactive bash | this box's default `PS1` embeds its own `\[\e]0;…\a\]`, so a one-shot title is overwritten by the very next prompt within milliseconds | **not fixable in the script** — neutralise `PS1`/`PROMPT_COMMAND` first, or let a long-running foreground process own the title |
| reusing a `TILLER_WL_LABEL` another run already used | **every path derives from the label**, the database included (`/tmp/$LABEL.sqlite`), so a repeated label silently inherits the previous run's *state* — one pass opened with the Projects sidebar full of dozens of unrelated projects from elsewhere in this engagement. Nothing is corrupted and writes stay inside that label's own file, but a fixture you believe is clean is not, and any count or "is it there after restart" claim taken from it is worthless. Found under labels `tpty1248`/`tpty1248b`, 2026-08-20 | **document**: derive a label unique to the run (a pid or timestamp suffix), never a short mnemonic that reads like a topic. If a sidebar shows anything you did not put there, stop and reissue under a fresh label rather than reasoning about it |
| a `ctl`/`shot`-only invocation left up with `TILLER_WL_KEEP=1` | the script starts the virtual pointer and keyboard **only when the action string itself mentions an input verb** (`click`/`move`/`type`/`chord`/…). An invocation of nothing but `ctl` and `shot` therefore boots an instance with *no input devices bound at all*; a later `chord`/`type` sent to that still-live instance reaches nothing and exits 0, which reads exactly like "the chord does nothing in the app". Found on the F-CORE-WSP-05 divider drive, 2026-08-20 | **document**: if you intend to keep an instance for follow-up input, put at least one input verb in the first invocation's action string, or tear down and reissue one that has both. The same silent-success shape as the expired-keyboard row above, so check this *first* when a chord appears inert |
| the harness never set **`TILLER_PROJECTS_DIR`**, so drives wrote into `$HOME` | unset, the app falls back to its own default of `$HOME/Tiller/projects` — the user's **real** home — so any drive that creates or clones a project writes there. Not hypothetical and not one agent's slip: fifteen directories from earlier verification rounds had accumulated there across two days before anyone noticed, and on 2026-08-20 a clone landed there again when a single Bash call started without the override. That is the shape of the failure — every tool call is a fresh shell, so a convention that each caller must remember cannot hold, and the runs it misses are precisely the ones nobody is thinking about | **fixed** — defaults to `/tmp/$LABEL-projects`, created up front, alongside every other label-scoped path. An explicit value from the caller is always respected, so a row that is genuinely *about* default placement can still point at `$HOME/Tiller/projects` deliberately; the startup banner prints whichever is in force, so that choice cannot be silently wrong in either direction. Verified both ways |
| this app's text fields drop `wtype`'s multi-character `type` | one drive could not get text into the clone-URL field with `type <text>` at all, and had to send `key <name>` one character at a time with ~0.2 s between presses. Distinct from the debounced-persist row below, which is about a value arriving *late* — here the characters never arrive. A field that stays empty reads as "the click never focused it", which is the wrong diagnosis. Found on the F-GIT-RUN-01 streaming drive, 2026-08-20 | **document** — if `type` leaves a field empty, fall back to per-character `key` before concluding anything about focus or about the control. Not every field behaves this way: `type` into the Project Settings display-name field on the same day delivered its full string, so test the field you are on rather than assuming either behaviour |
| **there is no attach mode** — a second invocation restarts everything | every invocation kills and relaunches the compositor and the app, including under `TILLER_WL_KEEP=1`. So a gesture chain cannot be split across invocations: the app you drive in call 2 is a *different process* from the one call 1 left running, with the context menu closed, the sheet dismissed and any modal gone. Worse for portal work — the new sway picks a fresh `wayland-N`, orphaning a portal fix bound to the old one. Two independent passes hit this on 2026-08-20: one lost a context menu between calls and had to fold the whole sequence into a single action string; the other, unable to do that for a `TILLER_WL_KEEP` portal session, wrote a throwaway driver copying `pointer_command`/`click`/`rightclick`/`shot` **verbatim** out of this script and pointing them at the live session's existing FIFO and socket | **fixed, 2026-08-20 — `TILLER_WL_ATTACH=1`.** Set it alongside the same `TILLER_WL_LABEL` a `TILLER_WL_KEEP=1` session is still running under, and the script skips the start-of-run pre-kill and the compositor/app boot entirely: it re-derives the compositor's wayland display from `$SWAYLOG` (nothing else survives across invocations — `KEEP` leaves the *processes* running, not this script's own shell, so no variable does), verifies the app and compositor are actually alive by the same environment-matching `kill_ours` already uses (never by name alone), and reuses the existing control socket, virtual-pointer FIFO and virtual-keyboard as found. It never tears anything down on exit, `TILLER_WL_KEEP` or not — an attach run must not disturb the session it attached to. **Verified, all four claims**: (1) process identity — the app pid read back after attaching (`find_ours TILLER_SOCKET … tiller`) was the exact same pid across five consecutive attach invocations, `3196696`, confirmed independently via `pgrep -f` each time; (2) a real cross-invocation gesture chain — call A (`click` to focus a terminal pane, `rightclick` to open its context menu, `shot`) exited, then a *separate* call B (`click` on "Clear Terminal") landed on the still-open menu and cleared the pane, both against pid `3196696`; (3) the failure mode — attaching to a label with nothing running exits `7` with `FAIL: … nothing to attach to`, no shot taken; attaching to a `KEEP` session that was itself started with only `ctl`/`shot` (no pointer verb in its own first action string, so it has no virtual-pointer FIFO at all — the exact silent-success trap two rows below) and then requesting `click` also exits `7` with an explicit "never started with a pointer verb" message, rather than silently reaching nothing; (4) the normal (non-attach, non-`KEEP`) path is unregressed — a plain invocation with `click`/`type`/`shot` still boots, drives and fully tears down exactly as before. `shot` also works unmodified when attached (proven in every one of the above). One correctness fix needed along the way: `pointer_command`'s ack wait is a bare `grep -qx "$id" "$VP_ACK"`, and a fresh invocation's `POINTER_COMMAND_ID` used to restart at 1 — colliding with a stale `1` an *earlier* invocation already left in the same FIFO's ack log, which would have returned success without ever waiting for the new command. Attach mode now seeds `POINTER_COMMAND_ID` from the highest id already in `$VP_ACK`, so ids stay unique across the whole life of the FIFO, not just within one invocation. |
| the app **inherited the caller's cwd**, and adopted a real checkout as a project | the script never `cd`'d before exec, and `initial_working_directory()` (`main.rs:11445`) walks *ancestors* for the nearest git repository. Invoking from the repo — the natural place to invoke it from, so this was the default path, not an edge case — brought the instance up with a live checkout registered as a project, and any terminal pane opened in it would run its shell **inside that tree**. Measured before the fix, launched from `…/tiller-linux`: the registered project was `/home/enzopalmisano/Scrivania/Progetti/tiller`, the **main worktree on a different branch** (`rust/gpui-rewrite`) — not even the checkout the drive was launched from, because the ancestor walk resolves a linked worktree to its main repo. A verification instance able to open shells in the tree under verification is the same contamination class as a sibling's `git add`. Found by a builder whose drive auto-registered the shared checkout, 2026-08-20 | **fixed** — the app now launches from an empty, label-scoped, non-repo directory (`/tmp/$LABEL-cwd`), so the ancestor walk finds nothing and stops there; `exec` in the subshell keeps `$!` the app's own pid. Verified before/after from the repo root. If you invoke the app outside this script, `cd` somewhere neutral first |
| reading `/tmp/$LABEL.sqlite` **without its `-wal`/`-shm` neighbours** | the DB runs in WAL mode (`tiller_persistence/src/db.rs:142`), so a committed write lands in the sidecar and **the main file's mtime never advances until a checkpoint**. A bare-file read — copied away from its sidecars, or taken after a `SIGTERM`, which runs no checkpoint because there is no handler — returns a *pre-checkpoint snapshot* that looks exactly like "the app never wrote it". Measured: two `project.add`s committed, live read sees both, bare read sees **zero**, main-file mtime identical to the microsecond across all of it. This produced a full outcome-2 defect report against F-PER-07 that was wrong; see `fullapp/VERIFY-per07-read-protocol.md`, 2026-08-20 | **document**: read the DB **in place, sidecars present**, with a read-only URI (`file:…?mode=ro`) — opening it read-write checkpoints the WAL and silently repairs the staleness you are trying to detect. And when verifying a *null* result, the control must be **younger than the effect**: a column written at add-time is populated in the stale snapshot too, so it proves the row exists and nothing more. Issue a fresh, undisputed write *after* the baseline and check the read sees that |
| reading a settings field back **immediately** after typing it | the persist is debounced and lags the keystrokes. Typing `Renamed Alpha` and reading at once returned `display_name='Re'`; after Close, `'Ren'`; with **no further input**, later reads returned the full `'Renamed Alpha'`. Same surface symptom as the WAL row above — a field that looks unwritten — from an unrelated cause, and getting the sidecars right does not fix it. Found on the F-PER-07 re-verification, 2026-08-20 | **document**: a single prompt read measures your own timing, not the app. Read twice with a gap and require the value to be *stable*, not merely present; treat a value that is still changing between reads as "not settled yet", never as truncation or loss |
| `shot` **between** a `down` and its matching `up` | `shot` forces a real window resize (nudge to W2xH2 and back), not a passive snapshot; two independent critics interleaved it while diagnosing the "+" tab-strip menu and each read the corruption as an app defect ("the menu closes on mousedown alone", "clicking never creates a tab") — see `PLUS-MENU-INVESTIGATION.md` | **document, not fixable in the script**: capture state before a gesture and after it completes, never mid-gesture. A real (if narrow) app bug this exposed — an open popup's anchor position lags one render behind a layout change that moves it — is fixed in `tab_bar.rs` |

**`TILLER_WL_BIN` pins the binary under test.** A critic judging a wave must not have the binary
swapped under it by a builder rebuilding the shared `rust/target`:

```bash
cp rust/target/debug/tiller /tmp/mylabel-tiller && export TILLER_WL_BIN=/tmp/mylabel-tiller
```

### A legal ACP counterparty for states the installed agent cannot produce

`rust/crates/tiller_ui/tests/fixtures/chat_fixture.py`, selected with `TILLER_ACP_PROGRAM`, is a
scriptable ACP v1 agent. It is the right instrument when the blocker is **upstream in the agent**
rather than in Tiller — measured example: `@agentclientprotocol/claude-agent-acp@0.69.0` only
forwards thinking chunks that carry text, and recent models default `thinking.display` to
`omitted`, so `agent_thought_chunk` is never emitted at any effort level. It stands in for an agent
this box lacks exactly as `Scripts/xdnd-source` stands in for a file manager: the app, the render
and the click stay production. **Say when you used it**, and never use it to stand in for a gesture.

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

**Proven end-to-end against a live app target (P130, 2026-08-16)**, closing the gap this section
used to flag. `file_view.rs`'s platform-click-opens-a-markdown-link path (`F-CORE-FILE-04`) is
gated on `event.modifiers.platform` at both `MouseDown` and `MouseUp`, resolves to a real
repo-relative file, and its "open a new tab" outcome needs no external process — the sharpest
available positive control. Same coordinates, same pane, same running instance: a plain `click`
left the tab strip unchanged (negative control); `modclick logo` opened the linked file as a new
foreground tab (positive control) —
`reference/linux-progress/p130-modclick/p130-negative-control-plain-click.png` and
`p130-positive-control-modclick-logo.png`. `TILLER_WL_PROTOCOL_LOG=1` (new env var, wraps the app's
own process in `WAYLAND_DEBUG=1`) independently confirms the wire order: `wl_keyboard.modifiers`
carrying `Mod4` (Logo) lands 25 microseconds to 64 milliseconds before the `wl_pointer.button` press
it is meant to gate, in both a terminal-link attempt and the markdown-link attempt above, and the
hold outlives the whole press/release pair every time. Full evidence and wire-trace excerpts:
`docs/linux-rewrite/tasks/P130-modclick.md`.

That leaves `F-TERM-UI-02` (cmd/logo-click a terminal URL) still not opening a Browser tab live,
but P130's evidence narrows why: it is not `modclick`, and not `F-CORE-FILE-04`'s old dead-code gap
either (`Workspace::add_file_tab` now subscribes to `FileViewEvent::OpenFile`, fixed independently
of this lane). The remaining candidates are app-side coordinate/hit-testing questions specific to
the terminal's own multi-pane layout — see P130's write-up for what to check next.

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

## Every frame is now captured at one fixed size (1715x972)

`shot()` used to force the lazy repaint by **alternating** the output between 1715x972 and
1400x900, leaving each capture at whichever size came next. That made the lane an instrument whose
scale changed between observation and action: coordinates read off one frame were sent while the
output was at the other size, `pointer_command` scaled them by the then-current `OUTPUT_W/OUTPUT_H`,
the click landed in dead space, and the row got written up as a missing feature. It cost at least two
false negatives in wave F (`F-EDIT-10`, `F-EDIT-11`, both later corrected to PASSED) and it spanned
invocations, because the alternation counter resets every run while the compositor keeps the size the
previous run left behind.

`shot()` now nudges to the off-size and returns immediately, so the configure still fires but every
captured frame — and every click, this run or the next — is in one coordinate space.

Controls run when this changed, all three of which must keep holding if you touch `shot()`:

- **Scale is fixed.** Four consecutive frames all reported `1715x972`.
- **Frames are still fresh.** Those same four frames had four different md5 sums, with colour counts
  tracking the real UI changes (6493 → 6771 → 8741 → 7958). A repaint fix that returns stale frames
  is worse than the bug it replaces — check this, not just the size.
- **Coordinates land.** With `Terminal` selected, a `click 378 51` read off the previous frame
  selected the `Chat` tab: tab underline moved, sidebar highlight moved, composer rendered.

Read coordinates off the most recent frame and they are valid. You no longer need to reason about
which size a frame was taken at.

## The capture pipeline was stress-tested and is not stale (P131, 2026-08-16)

A wave-G critic judging `F-CHAT-20` reported mid-stream captures "seconds apart" showing identical
progress percentages, and called it "consistent with a capture-pipeline staleness artifact in this
harness." That claim would have weakened every null result in this project, so it was tested directly
rather than assumed — with a driven, predictable animation (a terminal counter ticking every 300 ms,
whose expected value at any capture time is computable from wall-clock brackets) instead of a live
agent stream, which has no ground truth to catch a stale frame against.

**Result: no staleness found.** Three spaced `shot()` calls, five back-to-back `shot()` calls, and
raw `grim` calls with **no resize nudge at all** (both against the ticking terminal and against a
one-shot `project.add` socket mutation — the exact scenario trap 2 below describes) all returned
frames that tracked real content, converging to fresh state within roughly the same ~0.6-2 s margin
`shot()` already budgets. Full experiment design, every MD5/timestamp table, and the screenshots are
in `tasks/P131-capture-staleness.md`.

**This also narrows trap 2, immediately below.** Its blanket claim that grim returns a stale frame
"byte for byte" absent a forced configure did not reproduce on the current build — an un-nudged
`grim` call converged to the correct, fully-settled frame within ~2 s with no resize anywhere in the
sequence. Read trap 2's *lesson* (verify a negative by forcing a repaint before you believe it — cheap
insurance, and `shot()`'s nudge remains a strictly stronger, deterministic guarantee than hoping
ambient settle time was enough) as still sound; read its literal claim that an un-forced capture is
permanently stuck as **not current**, per the un-nudged tests in P131.

**What this does *not* rule out**: whether the specific `context-ring` percentage widget itself calls
`cx.notify()` on every `ContextUsage` update is an app-side (`rust/`) question this task's read-only
instrument cannot answer, because exercising it needs a live ACP event — the same untrustworthy
instrument this task replaced. The likelier read of the original report is that the agent's own
`UsageUpdate` cadence was coarser than the sampling interval, i.e. the value had genuinely not
changed yet. Do not re-litigate a row as "harness staleness" without a P131-style driven-animation
control of your own — a live stream cannot tell you which one you're looking at.

## `xdnd` — a REAL compositor-delivered XDND drag source (P132, 2026-08-16)

`F-CORE-FILE-03A` was `half-proven`: the app's side of a file drop was already traced correct end to
end against the vendored pinned Zed `gpui_linux` checkout, and a pre-existing test
(`a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files` in
`tiller_terminal/src/lib.rs`) drives a real two-file ordered drop through GPUI's own simulated-drag
harness and passes. What was missing was a genuine compositor-delivered `wl_data_device_manager`
drag — not GPUI's in-process stand-in.

**`Scripts/xdnd-source`** is that missing half: a standalone Rust crate (`wayland-client` +
`wayland-protocols-wlr`), deliberately kept OUTSIDE `rust/`'s Cargo workspace so it never touches the
app's `Cargo.lock` or build — it becomes a second Wayland client on the same nested-sway connection
and acts as a real XDND drag **source**, offering `text/uri-list`. `wayland-drive.sh` wires it in as a
new `xdnd <x1> <y1> <x2> <y2> <file...> [--delay-ms N]` action, built on first use
(`cargo build -q` inside `Scripts/xdnd-source`, cached after that).

Mechanics: `xdnd-source` maps a tiny (10x10px) `zwlr_layer_shell_v1` **overlay-layer** surface at
`(x1,y1)` — deliberately not an `xdg_toplevel`, so sway's tiling never touches Tiller's own window or
the coordinate space every other action in this script already relies on. It prints `READY` once that
surface is configured and eligible for pointer focus. The driving function then presses the *same*
persistent virtual pointer `click`/`drag` already use (P124) at `(x1,y1)` — a real `wl_pointer.button`
press the compositor delivers, which is the only legitimate source of the serial `wl_data_device
.start_drag` requires (a serial cannot be forged from a separate, unrelated process). `xdnd-source`
prints `DRAG_STARTED` once that request is sent, the driver then walks `steps` real intermediate
`move` waypoints to `(x2,y2)` and releases — the compositor delivers `wl_data_device.enter/motion
/drop` to whatever surface is now under the pointer, i.e. Tiller's own window, exactly as a real file
manager's drag would.

**Proven live, ordinary speed**: two files
(`/tmp/xdnd-test-a.txt`, `/tmp/xdnd-test-b.txt`) dragged from `xdnd-source`'s overlay onto a live
Terminal pane landed, in order, shell-quoted and space-joined, in the pane's real prompt —
`'/tmp/xdnd-test-a.txt' '/tmp/xdnd-test-b.txt'` — which is `tiller_project::terminal_file_drop`'s
exact output shape, reached through the production `on_drop::<gpui::ExternalPaths>` handler at
`tiller_terminal/src/lib.rs`, not a test-only fixture. `xdnd-source` itself reported the full
protocol sequence — `DRAG_STARTED` → `TARGET text/uri-list` → `ACTION Copy` → `SEND` →
`DROP_PERFORMED` → `FINISHED`. Screenshots:
`reference/linux-progress/p132-xdnd/01-before-real-xdnd-drop.png` and
`02-after-real-xdnd-drop-two-files.png`.

### A real bug this instrument found and fixed in itself: destroying the self-offer cancels every drag

The first working version of `xdnd-source` printed `DRAG_STARTED` and then `CANCELLED` within
**tens of milliseconds**, before the driver script had sent a single `move` waypoint. `WAYLAND_DEBUG=1`
on `xdnd-source`'s own connection explained it: the pointer is still over `xdnd-source`'s *own* tiny
overlay for the first instant of every drag (`start_drag` fires before any motion), so the compositor
self-delivers a `data_offer` + `enter` to `xdnd-source`'s own `wl_data_device`, treating it as a
candidate drop target. The first version's `Dispatch<WlDataDevice>` handler called `id.destroy()` on
that self-offer the moment it arrived; wlroots reads an unaccepted, destroyed offer on the *origin*
surface as "no one will ever take this drag" and cancels it outright — visible on the wire as
`wl_data_device.enter/motion` immediately followed by `leave` + `wl_data_source.cancelled`. The fix
(now in `Scripts/xdnd-source/src/main.rs`) is to do nothing at all with that self-targeted offer —
matching `gpui_linux`'s own `DataSourceKind::Drag` handler, which likewise treats `dnd_finished` and a
trailing `cancelled` as interchangeable teardown signals and takes the first one it sees.

### The "slow-resolving provider" clause — simulated, and it found a real race, now patched

**Update, 2026-08-18:** the race described below is fixed — see `rust/vendor/README.md` and
`docs/linux-rewrite/tasks/P133-gpui-xdnd-slow-provider-race.md`'s update note. `--delay-ms 400`
now lands the drop correctly, same as `--delay-ms 0`; the paragraph below is kept for how the bug
was found and diagnosed, not as current behavior.

### (historical) The "slow-resolving provider" clause — simulated, and it finds a real race

On `text/uri-list` the whole file list arrives through **one pipe in one write** (that is the
substance of the "one pipe in one background task" half of `F-CORE-FILE-03A`'s diagnosis) — there is
no per-file async resolution to be slow about, unlike macOS's `NSItemProvider`, which resolves each
dragged item independently and can race a UI timeout per item. `--delay-ms N` simulates the closest
analogue this MIME type has: it delays `xdnd-source`'s write into the offer pipe until N ms after the
target's `receive()` request triggers the `send` event. **Whether that is the same hazard the macOS
clause was written for is not proven here** — it is a single-shot delay on the one and only write,
not a per-item race, and that difference should be weighed by whoever reads this next.

What it found instead is real: **at `--delay-ms 400` and `--delay-ms 2000`, the drop is silently
lost.** No file paths ever reach the terminal prompt
(`reference/linux-progress/p132-xdnd/03-slow-provider-400ms-drop-lost.png`, prompt empty), the app
logs no error at all, and `xdnd-source` itself never receives `dnd_finished` *or* `cancelled` — it
times out after its own 15s safety net. This is well inside the app's declared
`PIPE_READ_TIMEOUT` (4s, `gpui_linux`'s `linux/platform.rs`), so that timeout is not what fires.
Reading `client.rs`'s `wl_data_device::Event::Drop` handler explains it: it bails out immediately
(`let Some(drag_window) = state.drag.window.clone() else { return; };`) unless `Enter`'s **async**
pipe-read task has *already* completed and populated `state.drag.window` — and that task only starts
reading once `Enter` fires, which in this drive happens roughly 150-350ms before the scripted
button release (`Drop`) reaches the app, an interval this MIME type's normal (`--delay-ms 0`) case
comfortably wins and a few hundred milliseconds of provider latency does not. Once `Drop` bails,
`data_offer.finish()`/`destroy()` are never called, so the compositor has nothing to tell either side
the transfer is over — the paths are silently discarded even after the async task eventually finishes
reading them, because by then no `Drop` event will ever fire again to consume the result. This lives
entirely in the vendored, pinned `zed-industries/zed` checkout (`crates/gpui_linux/src/linux/wayland
/client.rs`), not in any file this repository owns, so it is recorded here rather than patched.

### Using it

```bash
Scripts/wayland-drive.sh /tmp/shots '
  ctl project.add path=/abs/path/to/a/project
  xdnd 6 6 700 400 /tmp/some-file.txt /tmp/another-file.txt
  shot after-drop
'
```

`(x1,y1)` is where `xdnd-source`'s tiny overlay is anchored (top-left margin) and where the driver
presses; `(x2,y2)` is the drop target — pick a point inside a live Terminal pane's `.size_full()`
drop-target region to exercise `F-CORE-FILE-03A`/`F-TERM-PTY-06`'s `on_drop::<gpui::ExternalPaths>`
path. Each `<file>` is turned into a `file://` URI. Add `--delay-ms N` to simulate a slow provider
(see above — this used to lose the drop for any interval past roughly 350ms; fixed 2026-08-18,
see `rust/vendor/README.md`). `xdnd`'s own log lands
in `<label>-input/xdnd-source.log` (same directory the persistent pointer/keyboard logs already use)
and is echoed to the driver's own stdout on both success and failure.

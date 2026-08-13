# Linux/waku rewrite — state of play

Written by the orchestrator so this work survives losing any single session. Anyone picking it up
should be able to read this file and continue without re-deriving anything.

Branch `linux/gpui-waku`, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`.
Nothing is committed yet — the whole state below lives in the working tree.

---

## The goal, in one line

Tiller rebuilt in Rust on native GPUI, now targeting Linux as well as macOS. No webview, no HTML.
Features stay Tiller's; the UI is redrawn taking inspiration — never code — from waku.
**Done means:** a critic that did not build a piece exercises every inventory entry live and ticks it.

## How far along — read `INVENTORY-STATUS.md`

The two inventory files still hold 388 unticked boxes; verdicts accumulated in the critic's report
and in pane replies for thirteen hours with nothing consolidating them.
`docs/linux-rewrite/INVENTORY-STATUS.md` now does, and it keeps the one distinction the goal turns
on: **critic-confirmed** (≈50 — the only ticks that count toward done) versus **builder-claimed**
(≈38, awaiting an independent pass). Roughly 250 entries are still untouched, most of them domain
logic in `02-inventory-packages.md`, which is headless by construction and therefore the largest
reachable work while no display exists.

## The four references, frozen

| File | What it is |
|---|---|
| `01-inventory-app.md` | 217 verifiable capabilities from the Swift app target |
| `02-inventory-packages.md` | 171 from the domain packages |
| `03-visual-bar-and-gpui-patterns.md` | waku's measured visual system + GPUI idioms + Linux windowing |
| `00-ui-observed-from-screenshots.md` | the old macOS UI read from `reference/shots/*.png` |
| `04-ux-patterns-waku-does-not-cover.md` | orca/t3code, for diff and surfaces waku lacks |

388 inventory entries total. That checklist is the contract.

Reference checkouts (read-only, never copy code): `../\_tiller-refs/{waku,zed,orca,t3code}`.

## Who owns what right now

| Pane | Piece (as of 14:30) | Files it owns |
|---|---|---|
| `pi` (w1:p4) | P44 — the chat surface, unexercised since 01:00 | `tiller_ui/**`, `tiller_theme/**` |
| `codex11` (w1:p2) | P45 — get the verification gate green again | everything not listed for the others: `tiller_terminal`, `panes.rs`, `tiller_activity`, `tiller_persistence`, `tiller_project`, `tiller_usage`, `tiller_markdown`, `tiller_acp`, `tiller_agents`, its `Scripts/*` |
| `codex12` (w1:p3) | P43 — the shell command layer, then the absent tab operations | `tiller_control/**`, `tiller/src/main.rs`, project discovery, `tiller_git/**` |
| `pireview` (w1:p6) | critic pass 9 — building `INVENTORY-LEDGER.md`, one row per entry | writes `CRITIC-baseline.md`, append-only |

**The single biggest gap that is not the display (critic, pass 8):** the shell has **no menu bar and
no command layer at all**, and that one absence explains **19 of the 29 absent non-browser
features**. The only shell interaction surface is the tab strip plus the `+` menu. Nineteen entries
had been counted as nineteen gaps; they are one gap counted nineteen times.

**Two things belong to the operator and nobody else can do them:** restart the compositor (which now
unblocks only *appearance*, since interaction was recovered via GPUI's test harness), and decide
whether to commit — 6600+ lines and the whole of `docs/linux-rewrite/` are still outside git.

Unowned and claimable: `tiller_usage/**`, `tiller_project/**`, `tiller_persistence/**`,
`tiller_git/**`, `tiller_acp/**`, `tiller_markdown/**`. An agent that claims one should say so in
its reply so this table stays true.

Briefs live in `docs/linux-rewrite/tasks/`. They are written to be **self-contained**, because
every agent's context is reset before it is given one (see Method).

Ownership is by **file**, not by feature. Three agents in one worktree will otherwise overwrite each
other silently — not as a clean git conflict, but as one agent reading a file, thinking for two
minutes, and writing over another's work.

## Closed

- **Workspace resolves and compiles on Linux.** `gpui`/`gpui_platform` were absolute path deps into
  a macOS checkout; now a pinned git rev (zed `c05e346`).
- **The app renders.** `gpui`'s `wayland`/`x11` features are on by default, so `default-features =
  false` silently removed them: `guess_compositor()` then answered "Headless" and the app ran a
  healthy event loop with no window, no error, no crash. Both crates now enable them explicitly.
- **Dark by default on Linux** (F-002). Was Light, because GPUI resolves appearance through the XDG
  portal and defaults Light without one.
- **XDG paths** (F-003). Was writing to `~/Library/Application Support` on Linux.
- **Control socket** (F-001). Serves six methods: `system.ping`, `system.capabilities`,
  `system.identify`, `workspace.list`, `workspace.current`, `notify`. Everything else is absent.
- **Selection colour** (F-005), except the Files/Changes segment.

## THE SECOND UNBLOCK — GPUI can test the UI with no display (found 13:20)

`gpui::TestAppContext` with `VisualTestContext` runs the **real element tree** with no display,
produces a **drawn frame with a debug-bounds map**, and dispatches **real mouse and keyboard
events through the real dispatch path**. Elements are found with `.debug_selector(id)` and clicked
at their actual laid-out bounds. Working examples: `tiller_ui/src/changes.rs` tests.

| | |
|---|---|
| **Proves** | the element exists in the laid-out frame at real bounds and responds to a real click or key — layout, presence, hit-testing, **interaction** |
| **Does not prove** | what the pixels look like: colour, font rendering, the comparison against waku |

**A large part of what was written off as `NOT EXERCISED — blocked on display` was never about
display — it was interaction.** Clicking a tab, pressing a chord, double-clicking a file, opening a
menu: all reachable today. Appearance stays blocked and stays a debt.

**Why this went unnoticed for thirteen hours.** The blockage was diagnosed correctly and every
response to it was about getting a *picture* out of the running app — other displays, software
Vulkan, native Wayland, `grim`. Nobody asked whether the thing being verified required a picture.
A dozen entries needed a click, not a photograph. *"We cannot see the app"* quietly became *"we
cannot test the UI"*, and the second does not follow from the first.

The rule, alongside "make the state observable rather than lowering the standard": **first check
whether the evidence you think you need is the evidence the question actually requires.**

## Open

Refreshed 13:25. Findings are hypotheses with timestamps; several below were retired by looking.

| # | Finding | Status |
|---|---|---|
| F-007 | **The "silent crash" is a presentation freeze, and probably not ours.** Pixels stop; process, threads and control socket stay healthy. Thread state during a freeze: main in `poll_schedule_timeout`, workers in `futex_do_wait` — waiting for a frame callback, not spinning or stuck in a GPU ioctl. A wedged display server produces exactly this. **Unproven either way until the environment is healthy** — this was softened once before on weaker evidence and that was wrong. | open, untestable today |
| — | **No display presents.** See the 10:35 section. **Appearance** cannot be exercised — but interaction now can, see the section above. | needs the operator |
| F-GIT | 6 entries FAILED for absent behaviour: streaming runner, `branch --list`, clone, remote parsing, directory-status aggregation, side-by-side. | unassigned |
| F-CHG | The mutation tier had no socket door, so nobody could exercise stage/unstage/discard headless. | codex12 (P37) |
| — | `stage` succeeds on a conflicted path and stages the conflict markers; the Swift original refuses. | codex12 (P37) |
| — | Storage layer never tested for migration, corruption or concurrency — every persistence claim today rests on it. | codex11 (P38) |

**Closed since 10:40**, each with the evidence its brief demanded: **F-010** Changes mounted as a
first-class tab with a shell-level mount guard · **F-008** badges derived from
`discover_availability()` · **F-004** Permissions gated off non-macOS · **F-011** the editor now
exists · **F-012** projects no longer breed (`projects=1 worktrees=2` stable across three restart
cycles) · **F-PER-01** scrollback captured on quit and replayed on restore (nonce survived two
relaunches) · **`session.ref`** now in SQLite · **F-CHG-09** the dead error path — a removed `.git`
now surfaces git's own message with a working Retry, proven by a real click in a drawn frame · the
`on_action` wiring and socket doors for the command layer.

### Earlier findings, now closed

**Closed since the first pass:** F-001 socket · F-002 dark default · F-003 XDG paths · F-005
selection colour · F-006 SF Symbols (platform-gated, with `clamp_file_icons` correcting values
persisted on another platform) · F-009 worktree selection (`ControlState::current` now has a
writer; proven with `tillerctl` before/after and a 9303-colour frame) · P11 settings persistence ·
P16 three-section Changes + runtime mono font (**Fira Mono** here) · P17/P18 the whole automation
surface · P19 the affordances that did nothing (drag-to-reorder removed rather than faked).

**Retired by looking, at about two minutes each** — each would otherwise have cost a builder-piece:
the black strips in a frame (a root-crop artefact), "SF Symbols still offered" (stale frame), "the
`+` doesn't render" (visible in a 10:25 capture), `panel.list` blind to app panes and `panel.wait`
ignoring its timeout (both symptoms of F-009, retired when it was fixed).

### The characteristic failure of this codebase

Four instances, same shape: **a crate does the right thing and the surface does something simpler
and wrong.** `select_worktree` maintained state while the handler only re-highlighted; `ChangesTab`
had 38 tests and no mount; a runtime font token existed while five call sites named a macOS family;
`discover_availability()` reads `PATH` while the UI holds literals.

Every instance passes every unit test, because the tests test the crate and the defect is in what
the surface chose instead. **Unit tests cannot see this class.** The question that catches it, now
in every brief: *can a user reach this, and does it show what the crate actually computed?*

## Verified passing

ACP end to end (real agent, streamed reply, tool call, permission round-trip — proven with a nonce
and the file found on disk); Settings navigable; terminal accepts input and echoes output (nonce);
status bar shows real usage parsed from `~/.codex/auth.json`; control socket answers with real
workspace state.

## Environment facts — measured, do not re-derive

**Only the session's own Xwayland (`:1`) can present.** Vulkan needs DRI3 to present on X11.

| Display | DRI3 | Renders |
|---|---|---|
| Xvfb | no | window maps, every frame black |
| Xephyr `-glamor` | no | same |
| Xwayland `:101` rootless | **yes** | still black — contents go to the compositor, not an X drawable |
| Xwayland `:1` (real session) | yes | ~11.4k colours |

A capture with **one distinct colour** means presentation failed, not that the UI is empty.

**Input injection** only works via absolute screen coordinates and plain XTEST. The app is an X11
guest of a Wayland compositor, so X focus cannot be forced onto it and `xdotool --window` delivers
nothing. The cost: it moves the operator's real pointer. Keep runs short.

**Several instances of the app run on `:1` at once.** Any "largest window on the display" heuristic
will eventually photograph or type into another agent's build. Both scripts now match `_NET_WM_PID`.

**Installed agent CLIs:** `claude`, `codex`, `pi`. Absent: `opencode`, `omp` — entries naming them
are UNREACHABLE, not FAILED. Only `claude` speaks ACP; codex and pi expose no ACP entrypoint.

**Build note:** `cargo build -p tiller -p tiller_control`. `tillerctl` is a *binary* of
`tiller_control`, not a package; `-p tillerctl` fails and leaves you testing a stale binary.

## SOLVED 13/08 ~10:00 — why captures went blank, and the recipe that works

**Use an already-running display. Never create a fresh one.** Everything below is measured.

The compositor cannot import what the app renders:

```
cosmic-comp: Failed to render texture … import for wrong devices DrmNode { dev: 57984, ty: Render }?
             … modifier: Unrecognized(144115188076389125)
```

`57984` is `renderD128` — the only render node. `144115188076389125` is `0x0200000000000005`, whose
top byte `0x02` is the **AMD** vendor code: an AMD tiling modifier that cosmic-comp's renderer does
not recognise and therefore cannot import. This is a Mesa/compositor mismatch **outside this
project**; nothing in Tiller can fix it.

What makes it actionable: **Xwayland negotiates its dmabuf modifier set once, at startup, and keeps
it.** Displays created before the mismatch appeared still present. Displays created after it never
will. That is the whole difference, and it explains every confusing observation of the night.

| Display | Created | Result |
|---|---|---|
| three consecutive fresh `Xwayland` instances | now | **1 colour each** |
| `Xvfb` (also has no DRI3 at all) | now | 1 colour |
| `Xvfb` + software Vulkan (`VK_DRIVER_FILES=…/lvp_icd.json`) | now | 1 colour |
| native Wayland, no X at all | now | app runs, but `grim` cannot capture: cosmic-comp has no `wlr-screencopy-unstable-v1` |
| **`:1`** — alive since the session began | earlier | **8787–8916 colours at 1440x833** |
| **`:2`** — the critic's, alive for hours | earlier | 7729–8615 colours at 1920x1080 |

`:1` was found at 640x480 and enlarged with `DISPLAY=:1 xrandr --output XWAYLAND0 --mode 1440x900`;
it still presented afterwards, so **resizing does not lose the good modifier set.**

### CORRECTION 10:25 — the cutoff is later than stated above, and `:1` is dead

The shape of the explanation holds — displays created before a certain moment present, ones created
after it never do — but **the moment is not the 09:13 journal errors**, and `:1` is gone:

| Display | Created | Presents |
|---|---|---|
| `:1` | before the session | **died 09:56** |
| `:2` | **09:47:23** | **yes — 8820 colours at 1440x833, verified 10:25** |
| every display created after ~09:55 | now | no |

So the cutoff sits between 09:47 and 09:55, and `:2` — created *after* the journal errors — presents
perfectly well. Those errors were other clients failing, not the moment the door closed.

Exhausted since, all still blank on a fresh display: `MESA_VK_WSI_DEBUG=sw`; software Vulkan via
`VK_DRIVER_FILES=…/lvp_icd.json` on Xwayland (untested before — this was the most promising
candidate, because lavapipe produces linear buffers any compositor can import); the same on Xvfb.

**A likely cause, and it points at the fix.** `/dev/dri/` holds `card1` and `renderD128` — with
**no `card0`**. That renumbering is what a GPU device reset leaves behind. A compositor holding a
stale render node after such a reset would behave exactly like this: everything already connected
keeps working, nothing new can hand it an importable buffer.

**If that is right, the fix is outside this repo and needs the operator: restart `cosmic-comp`, or
log out and back in.** That is not something to do unasked — it is their desktop session.

**The recipe until then.** `:2` is the only presenting display, so everyone shares it. This is
workable but not free:

- Both scripts match `_NET_WM_PID`, so nobody drives or photographs a stranger's window. That
  filter is what makes sharing possible at all.
- `import -window <id>` reads the window's **own** pixmap and is immune to occlusion. The root-crop
  fallback is **not** — an overlapping window corrupts it silently. Three false "dead UI" verdicts
  already came from stacked windows on a shared display, so a root-crop frame taken while others
  are working deserves suspicion.
- **The critic has priority on `:2`.** Its live pass is the definition of done; builders keep their
  runs short and yield.

`Scripts/linux-shot.sh` used to create its own Xwayland whenever no display was named, which after
the mismatch appeared **guaranteed** a blank frame. It now defaults to `:1` and only creates a
private server when `TILLER_NEW_DISPLAY=1` is set, with a warning that says what will happen. The
old default is kept behind that flag for the day the compositor is restarted.

**If every display stops presenting**, the fix is outside this repo: log out and back in, or restart
`cosmic-comp`, which makes fresh Xwaylands negotiate a modifier set it can import again.

## Historical: capture on `:1` after the session's Xwayland restarted (superseded by the section above)

Around 03:00 the compositor replaced its Xwayland. Since then, on `:1`:

- `import -window <id>` returns a uniformly black frame — the app presents directly and keeps no
  window pixmap to read.
- `import -window root` + crop at the window geometry returns black as well.
- Same on a rooted Xwayland started by hand (`Xwayland :55 -ac`), with and without `-geometry`.

`GPUI_X11_SCALE_FACTOR=1` is still worth setting and is now in the script: without it the window
reported `3798x2152+-937+-536` — negative origin, double size — so any crop of that rectangle lands
off-screen. With it the geometry is sane (`1470x833+227+124`); the frame is simply still black.

Two paths that DO work and should be used until this is understood:

- **pireview** captures from its own long-lived `Xwayland :2 -ac` and gets valid frames (~2000
  colours at 640x480). Small, but enough to judge behaviour.
- **pi** captured `p8-chat.png` at the full 1470x833 with root-capture-and-crop and
  `GPUI_X11_SCALE_FACTOR=1`. The same recipe in `linux-shot.sh` returns black for the orchestrator,
  so something in the surrounding state differs — window visibility or stacking are the likeliest
  candidates and neither has been isolated.

`linux-shot.sh` now tries the window pixmap, falls back to the root crop, and **fails loudly with a
distinct message** if both come back flat. It will not report a black rectangle as a pass. That is
the important property; a capture that silently returns black is how "renders nothing" gets ticked
as working.

## Tools

```
Scripts/linux-shot.sh  <out.png> [settle] [display]              launch + capture
Scripts/linux-drive.sh <out.png> '<actions>' [settle] [display]  launch, drive, capture
```
`linux-drive.sh` exposes `click x y` (window-relative), `type "text"`, `key <keysym>`, `shot [path]`.
Both fail loudly rather than quietly: blank frames, unresolvable window origins and unmatched pids
are errors, not warnings.

## Method

- **A capability nobody exercised does not exist.** Ticks carry a nonce, a frame, or command output.
- **Measure pixels, do not describe them.** `convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]'
  info:`. Three confident visual readings were wrong tonight before this rule.
- **Check the whole set before doubting the witness.** Two near-miss false accusations came from one
  unrepresentative sample each.
- **Append, never rewrite,** in the critic's report. It lost three earned sections to a
  half-applied replace. Superseding findings get a new dated section.
- Snapshots of the critic's report land in `docs/linux-rewrite/critic-snapshots/`.
- **Reset the agent's context before every new piece** — `/new` for a `pi` pane (including the
  `pireview` critic), `/clear` for a `codex` pane. Each piece is meant to be individually
  judgeable, and carrying the previous piece's reasoning across blurs that boundary; for the critic
  it actively breaks the requirement that it starts fresh, unaware of how the thing was built. It
  also keeps panes off their context ceiling — pi reached ~84% and had to write a handoff to
  survive. **Collect the finished report before resetting: the reset discards it.** Order is
  always read → reset → dispatch, and the brief must then carry its own context.
- **A green build is evidence about types, not behaviour.** A compiler-enforced handoff guarantees
  the code exists to receive a call; only exercising it shows the call does anything (see F-009).

---

## 10:35 — every display is gone, and the headless route that keeps work moving

**`:1` died at 09:56. `:2` is wedged.** Its Xwayland process is alive and sitting in `ep_poll`, but
`xdpyinfo` from a fresh, unrelated client times out. The app instance running on it was killed to
test whether a client held a server grab; **the display did not recover**, so nothing is wedging it
from outside — Xwayland itself is stuck, blocked on a compositor that cannot import its buffers.
Displays created now do not present at all. There is no way back from inside this repo.

**The fix is a compositor restart, and it costs the session.** `Linger=no` and the herdr server runs
in `session-3.scope`, so a logout kills the server and every agent pane with it. The working tree is
on disk and survives; agent context does not — which is what these task briefs, `PI-HANDOFF.md` and
this file exist to replace.

### F-007 reframed a second time — the freeze is probably not ours

Thread state of the app while its pixels were frozen: main thread in `poll_schedule_timeout`, 12
workers parked in `futex_do_wait`, timer in `ep_poll`. **Not spinning, not blocked in a GPU ioctl.**
That is a well-behaved program waiting for a frame callback that never arrives.

Combined with a display server that stops answering a *fresh unrelated client*, which no bug in
Tiller's frame loop could cause, the evidence points at the display stack rather than the app.

**Stated as a hypothesis, not a verdict.** There may also be a genuine freeze in the app; it cannot
be tested until the environment is healthy, and it stays on the board as unproven either way. This
project has already softened F-007 once on weaker evidence and been wrong, so it is written down
with its status attached rather than closed.

### Headless is the route around it — verified, not assumed

```bash
export TILLER_SOCKET=/tmp/<your-own-name>.sock
env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller &
rust/target/debug/tillerctl current-workspace     # returns real state
```

The app stays alive with no `DISPLAY` and no `WAYLAND_DISPLAY`, the socket listens, and `tillerctl`
answers with real workspace state. `TILLER_SOCKET` isolates the endpoint, so **every agent can run
its own instance at once with no contention at all** — better than the shared display ever was.

Exercisable headless: every socket method, state changes, persistence across restarts,
worktree/project/tab management, **terminals** (PTYs need no display), and GPUI element presence
and interaction through `TestAppContext`/`VisualTestContext`. The latter draws the real tree and
dispatches real mouse and keyboard input without a display; it is evidence for behaviour, not for
pixels.

Blocked: claims that must be *seen* as pixels — layout, colour, font rendering, icon/status
treatment, visual comparison against waku, and the appearance half of mixed UI entries. Those are
**NOT EXERCISED — blocked on display**, never approximated and never ticked from reading the code.
An affordance is not display-blocked merely because it lacks a socket door: if the GPUI harness can
draw it and dispatch its click/key/drag, its behaviour is headlessly reachable and should be
tracked separately from its appearance.

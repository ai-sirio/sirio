# Linux/waku rewrite — state of play

Written by the orchestrator so this work survives losing any single session. Anyone picking it up
should be able to read this file and continue without re-deriving anything.

Branch `linux/gpui-waku`, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`.
Everything is committed — the working tree is no longer the state.

> **Head refreshed 2026-08-14 17:30.** Everything from "## Closed" down is kept as history and
> parts of it are superseded — in particular **the display crisis is over**; see
> "2026-08-14 — both lanes work" below before believing anything about `:1`, `:2` or blank frames.

---

## The goal, in one line

Tiller rebuilt in Rust on native GPUI, now targeting Linux as well as macOS. No webview, no HTML.
Features stay Tiller's; the UI is redrawn taking inspiration — never code — from waku.
**Done means:** a critic that did not build a piece exercises every inventory entry live and ticks it.

## How far along — read `INVENTORY-LEDGER.md`

`INVENTORY-STATUS.md`'s critic-confirmed/builder-claimed split is superseded.
**`INVENTORY-LEDGER.md` is the single source of truth**, one row per entry, and
`python3 Scripts/ledger-totals.py` is its gate — it recomputes from the body and asserts the totals
block matches. Run the gate rather than counting yourself; a paraphrased count has already produced
a phantom discrepancy here.

As of 2026-08-14 17:20 (389 rows, denominator frozen):

```
PASSED 190 · half-proven 34 · FAILED — absent 14 · FAILED — defective 47
UNREACHABLE 12 · N/A — platform 12 · NOT EXERCISED 79 · builder-claimed 1     (389)
```

Two things to read correctly:

- **`FAILED — absent` fell from 82 to 14 today** and that is the day's biggest correction. `P105`
  re-censused the bucket and found 20 rows built and 36 partial; the ledger had been asserting that
  features which exist do not. Only **14** are genuinely absent, and all 14 are dispatched.
- **`PASSED` did not move.** Removing unearned "absent" claims must never manufacture passes.
  Census evidence — a symbol and a line number — makes a row `NOT EXERCISED`, never `PASSED`.
  That substitution is the mechanism that produced this project's false passes.

`NOT EXERCISED` at 79 is now the real backlog, and `FAILED — defective` at 47 is the largest
actionable one. `P113-triage.md` groups those 47 by root cause and ranks them by rows-unblocked —
they are far fewer than 47 bugs.

## The four references, frozen

| File | What it is |
|---|---|
| `01-inventory-app.md` | 217 verifiable capabilities from the Swift app target |
| `02-inventory-packages.md` | 171 from the domain packages |
| `03-visual-bar-and-gpui-patterns.md` | waku's measured visual system + GPUI idioms + Linux windowing |
| `00-ui-observed-from-screenshots.md` | the old macOS UI read from `reference/shots/*.png` |
| `04-ux-patterns-waku-does-not-cover.md` | orca/t3code, for diff and surfaces waku lacks |

**389** inventory entries total, denominator frozen. That checklist is the contract. The 9 `F-BRW`
rows are in scope. COSMIC (Pop!_OS) supersedes waku as the general visual bar; the titlebar follows
the comet reference.

Reference checkouts (read-only, never copy code): `../\_tiller-refs/{waku,zed,orca,t3code}`.

## 2026-08-14 — both lanes work, and that changes the shape of the work

**The display crisis below is over.** There are now two ways to run the app, and the important
difference is not resolution but who may use them at once:

| lane | script | lock | input |
|---|---|---|---|
| **`DISPLAY=:1`** | `Scripts/linux-drive.sh` | **one holder at a time** (`mkdir /tmp/tiller-drive-1.lockd`) | real clicks and keys |
| **nested Wayland** | `Scripts/wayland-drive.sh` | **none — any number in parallel** | socket only (as of 17:30; `P112` is testing whether this can change) |

Read `ENVIRONMENT.md` and `WAYLAND-LANE.md` before either. The Wayland lane has five traps that have
each cost someone a false result.

**The single-holder X lock is the project's throughput ceiling.** Everything needing a gesture queues
behind one agent. That is why `P112` — can the Wayland lane be given synthetic input — is worth more
than the row it would close.

## Who owns what right now (18:20)

**The OpenAI account is exhausted, and it backed four of the seven panes.** Not a rate limit that
clears in minutes — `codex11` was told **2026-08-20 08:19**. Every `openai-codex` model in the
picker draws on it, so switching models within that provider buys nothing.

| Pane | Piece | Territory |
|---|---|---|
| `sonnet` (w1:p5) | `P109` — the 34 gestures `P106` could not reach; **holds the `:1` drive lock** | critic, drives only |
| `fable` (w1:pD) | `P111` — settings + adapter absent rows, two of which may be `N/A — platform` | `settings.rs`, `status_bar.rs`, `tiller_agents`, `tiller_usage` |
| ~~`codex11` (w1:p2)~~ | **OUT OF CREDIT.** Never started `P117` | — |
| ~~`codex12` (w1:p3)~~ | **OUT OF CREDIT.** Delivered `P118`; done `P110` | `sidebar.rs`, notification handler in `main.rs` |
| ~~`pi` (w1:p4)~~ | **OUT OF CREDIT** — died on the `P117` dispatch, 18:18. Delivered `P119`; done `P112`, `P116` Slice C | — |
| `pireview` (w1:p6) | `P116` Slice A non-chat 10 rows; done `P113`, `P115`. **Same account — expect it to stop mid-turn** | critic; **sole owner of `INVENTORY-LEDGER.md`** |

Only `sonnet` and `fable` are durable, and both are mid-task. **`P117` and `P120` are unowned with
nobody to give them to**, so the orchestrator is diagnosing `P117` directly; whoever fixes it,
someone else judges it.

### What was tried, so nobody retries it

`pi` (w1:p4) still holds `gpt-5.6-terra high` and a fresh `/new` context — the reset did **not**
downgrade it this time, and the picker was closed with `escape` without selecting. Filtering that
picker for `claude` returns **no matches**: no Anthropic provider is configured there. The only
non-exhausted models it offers are `opencode-go` (`minimax-m3`, `qwen3.7-max`, `qwen3.7-plus`),
which is a poor match for a subtle GPUI layout defect. Left as-is so it resumes when the limit
clears.

Answer any rate-limit or `/model` menu with `send-keys down`/`enter` and read the cursor back — a
typed digit confirms the highlighted default instead, which is how `codex11` got silently moved to
`gpt-5.6-luna high`.

**Ownership is by file, not by feature.** Two agents in one worktree otherwise overwrite each other
silently — not as a git conflict, but as one agent reading a file, thinking, and writing over
another's work. Commit path-scoped, never `git add -A`, and `grep '??'` before calling a piece done:
the rule that prevents collisions never catches a file nobody is thinking about.

Expect `index.lock` contention with six panes committing. **Retry; never delete the lock.**

### Standing rules that cost something to relearn

- **The critic must never be the agent that built the piece.** Rotate.
- **A feature the critic has not successfully tried does not exist.** Code plus a green test is
  `NOT EXERCISED`, never `PASSED`.
- **Reset context before every piece** — `/clear` for `codex` and Claude panes, `/new` for `pi`
  panes. **Check the model after `/new`**: it resets a pi pane to the global default and the critic
  silently loses its stronger model. Order is always read → reset → dispatch, and the brief must
  carry its own context.
- **Reproduce a gate with the gate's own command.** A paraphrase returned "clean" while the gate was
  red and overruled three correct agents.
- **`herdr pane send-text` replaces the buffer** rather than appending; `herdr pane run` sends text
  and Enter together and is what you want. `shift+tab` and bare `Enter` via `pane key` report
  success and do nothing.
- Every agent here is **text-only**. Visual judgement is the orchestrator's alone —
  see `CRITIC-visual-baseline.md`.

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

## 2026-08-14 17:40 — the finding that outranks the backlog

**A completed chat turn renders nothing.** The agent runs, answers, and finishes; the tab shows a
completed-turn ✓, the composer resets, the context ring falls to 5%, and `surface.chat.read` returns
`user → assistant("ORCHVIS") → turn`. **The transcript draws none of it.** Reproduced twice with
different messages on a binary containing every chat commit through `c23da36`.

This is why the project's rule exists: **134 tests pass over a transcript that draws nothing.**
Every row whose evidence is "the transcript shows X" is unprovable until `P117` lands, and any such
row already marked `PASSED` from a drawn test rather than a live frame should be treated as suspect.

A second content region has the same smell: the Changes list loads 50 real files and is **clipped to
about 150 px**, cutting its `Untracked` header mid-row. Whether that is one root cause or two is the
first question `P117` must answer. Full evidence, with capture paths, is in
`CRITIC-visual-baseline.md` under the 17:40 heading.


### 18:10 — the roster lost a pane, and one assumption needs retesting

**`codex11` hit its account usage limit and is gone until 2026-08-20 08:19.** It committed nothing
broken — `HEAD` was clean and it never modified `chat.rs` — but it also never got past planning.
`P117`, the empty-transcript fix and the highest-value item on the board, is **unowned and
effectively unstarted**. Its plan survives at `docs/superpowers/plans/2026-08-14-p117-transcript.md`
with every box unchecked; the proposed test name
`drawn_chat_transcript_gets_the_full_center_surface_height` was a hypothesis, not a diagnosis.
Reassign `P117` to the next pane that frees, and treat that plan as a starting point, not a finding.

`codex12` shares that account and is still running, so it may stop the same way without warning.
**A codex pane can die mid-edit**; when one goes quiet, check `git status` before assuming its work
landed.

**Assumption to retest: agents may not all be text-only.** `codex12`'s transcript contains
`• Viewed Image └ reference/linux-progress/p118-reset2-retry2/04-reset-settled.png`. If codex panes
really can see captures, they can close visual rows themselves and the orchestrator stops being the
only visual gate — which would change how every visual row is assigned. `CRITIC-visual-baseline.md`
currently asserts the opposite. **Do not act on this until it is tested against a capture whose
content is already known** — ask for text that is provably in the frame and check the answer.

## HISTORICAL — 2026-08-13's display crisis (superseded 2026-08-14)

Everything from here down was written while no display presented. **It no longer describes reality:
`DISPLAY=:1` works and a nested-Wayland lane exists.** It is kept because the measurements are real
and expensive — the DRI3 table, the dmabuf-modifier diagnosis, the blank-frame failure modes — and
because the reasoning is worth reading. Do not act on its recipes; use `ENVIRONMENT.md` and
`WAYLAND-LANE.md`.

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

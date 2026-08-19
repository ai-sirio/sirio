# Linux/waku rewrite — state of play

Written by the orchestrator so this work survives losing any single session. Anyone picking it up
should be able to read this file and continue without re-deriving anything.

Branch `linux/gpui-waku`, worktree **`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`** on an
x86 desktop. Everything is committed — the working tree is no longer the state.

> **Head refreshed 2026-08-18 evening. The machine changed a THIRD time** — off the Raspberry Pi,
> back onto x86. Read `ENVIRONMENT.md`'s 2026-08-18 section before anything else here: every
> absolute path in the Pi sections below is wrong, and **`DISPLAY=:1` is alive again**, which
> reopens the one lane the `F-BRW` bucket had nowhere to run.
>
> **The fan-out figure written here earlier (10) was wrong and was measured wrong.** Ten
> lane-driving agents took this box to load 41 with 232 MB of swap left. The real ceiling is
> **about 5 concurrent lane-driving agents** — see `ENVIRONMENT.md`'s concurrency section.

---

## 2026-08-19 (early hours) — 368 / 389, and no defective rows left

Waves N, O and P landed overnight. The ledger reads **368 PASSED**, and both hard-failure buckets
are now empty of `FAILED — defective`. What is left:

| | |
|---|---|
| open, actionable | **11** — 10 `half-proven` + 1 `FAILED — absent` (`F-GIT-RUN-01`, git cancellation) |
| blocked upstream | **3** — the `F-AGENT-OMP` rows, see below |
| `N/A — platform` | 7 |
| `NOT EXERCISED` | **0** — every row in the inventory has now been reached by at least one pass |

`F-TAB-09`'s native GTK file picker was the last row nobody had ever driven. It fell to the
private-portal recipe, and the same recipe closed `F-PRJ-14` and `F-PRJ-18`.

### The evidence standard rose: reproduce the defect, then confirm the fix

Wave O's critic judged all six of wave N's fixes on **both sides** — it rebuilt a pre-fix binary
from source (`git archive <commit>^ | tar -x`, never `checkout`/`stash`/`reset`), reproduced each
original defect on it, and only then confirmed the fix on a HEAD binary. This is now the standard
for judging any fix. The reason is simple: *a fix you only ever saw working cannot be told apart
from a feature that was never broken.* One reconstructed tree can serve several rows, provided you
check the commits are independent with `git diff --stat` first.

### A new failure mode: the builder that forgot it was the builder

`wf-wsp2` built two fixes early in a 110-minute run, then hours later judged them **believing a
predecessor had written them**, and passed them on that basis. Its report describes its own work as
someone else's. The commit timestamps (02:36, 02:58) fall inside its own run window and no other
lane held WSP rows, which is how it was caught.

The lesson is not "agents lie" — this one was scrupulous, and re-ran the tests rather than trusting
the report it thought it had inherited. It is that **the builder-cannot-pass-own-work rule needs an
external check**, because an agent's belief about who wrote a commit is not evidence. Verify
authorship against the run window before accepting any "a predecessor already fixed this".

### `F-AGENT-OMP-01/02/03` are blocked upstream, definitively

Re-probed across **every published version**, not just the installed one. `npm view oh-my-pi
versions` returns exactly `[0.1.0, 0.1.1, 0.2.0]`. The first two declare **no `bin` field at all**,
so no `oh-my-pi` executable exists. `0.2.0` declares one, but that file carries TypeScript
annotations under a `#!/usr/bin/env node` shebang (rejected by node v24.19.0 *and* by bun 1.3.14)
**and** imports `../src/*.ts` files the published tarball omits entirely — so even a TS-capable
runtime fails module resolution. `dist/` holds the package `main`, a library entry, not a CLI.
Tiller's adapter launches `oh-my-pi --hook <path>` (`omp.rs:77`) and the distribution ships no `omp`
alias. No local workaround can produce a runnable agent; do not spend another pass on this.

### One root cause, two very distant symptoms

The nested lane's file dialogs hung, and separately the host's own desktop flooded `/var/log/syslog`
at up to 72 MB/s. Both were the same bug: **a missing `XDG_CURRENT_DESKTOP` in the portal router's
exec environment.** `xdg-desktop-portal` picks its backend from that variable, so with it unset the
service runs "active" while registering *no* backend at all — `org.freedesktop.portal.Settings`
simply does not exist on the object, and every client polling it retries forever. Two agents found
this independently with different instruments (`dbus-monitor` in the lane, `gdbus`/`systemctl` on
the live session), which is what real corroboration looks like.

Fix in either context: read the true display from a live client's `/proc/<pid>/environ` (the
compositor itself has no `WAYLAND_DISPLAY` — it *creates* the socket), then
`dbus-update-activation-environment --systemd WAYLAND_DISPLAY=… DISPLAY=… XDG_CURRENT_DESKTOP=…`
**before** the first portal-triggering call. Repairing the backend does not silence clients that
were already running: their zbus connections have errored out permanently and only a session
restart clears them.

---

## 2026-08-18 — where this actually stands

Measured, not inferred:

| | |
|---|---|
| `cargo build --workspace` | **exit 0** |
| the app | **renders in full** under the nested Wayland lane — sidebar, tab bar, Chat/Terminal tabs, Files panel on the real repo, status bar with live Claude usage (`1715x972 · ~6500 colours`) |
| the ledger | **309 / 389 PASSED** after the day's re-census. Open: 55 `half-proven`, 13 `NOT EXERCISED`, 5 `UNREACHABLE`, 1 `FAILED — defective`, 0 `FAILED — absent`, 6 `N/A — platform` |
| agent CLIs | `claude`, `codex`, `opencode`, `pi` all installed — so **every `UNREACHABLE` parked on "not installed" is stale** |

Neither of the goal's two by-definition gaps is open: it compiles and it renders.

### The count went DOWN today, on purpose

It read 363 this morning and reads 309 now. Nothing regressed. Those 363 had been earned across
**three different hosts**; the code did not change under them, the platform did, twice. The goal's
bar is a full-app critic that ticks every entry *by exercising it live*, so the finish line was
never "close the last 26 rows" — it is a **full re-exercise of all 389 on this host**. Re-driving
replaces inherited belief with local evidence, and where the belief was unfounded the number falls.
A re-census that only ever raises the count is not measuring anything.

### Three ways a row got marked PASSED without being true — all found today

Every one of these was caught by a critic *driving the app*, and none by reading code or by
`Scripts/dead-models.py`. Check for them before trusting any row whose evidence is not a live drive.

1. **A green test over code the app never calls.** `tiller_project::layout`
   (`WorkspaceLayout`/`LayoutNode`/`PaneGroup`/`WorkspaceSnapshot`) has **zero references** in
   `crates/tiller` and `crates/tiller_ui`; only `LayoutCommand::Rename` is wired, from
   `commit_tab_rename` (`main.rs:8567`). Six rows rested on it. Same story for
   `classify_file_drop`, whose behaviour really lives in `chat.rs`'s `drop_external_paths`.
   See `WSP-LAYOUT-DECISION.md` — the dead half is a *second parallel model* the app already
   replaced, not an unfinished feature. **Before accepting a test as evidence, grep the app crates
   for the symbol.**
2. **Equivalence by assertion.** "Same production path as row X" is a claim, not evidence. Two such
   claims were rejected on inspection because the clauses genuinely differed (a working link that
   opens a tab is not the missing-file error path).
3. **A verdict outside the vocabulary.** `PASSED (resume leg environment-limited)` reads as a pass
   and is not one. Partly proven **is** `half-proven`, with the unproven leg named.

### Merging verdicts from parallel agents needs an explicit precedence

`NOT EXERCISED` and `UNREACHABLE` are **not verdicts** — they are absence of information. Merged
naively, last-writer-wins lets an agent that never reached a row overwrite one that drove it; this
nearly erased three live-earned passes. Rule now applied by the orchestrator: only rank-2-and-above
verdicts (`half-proven`, `PASSED`, `FAILED — *`, `N/A — platform`) are written to the ledger.
No-information results are withheld, and the ledger keeps whatever it already had.

Related: **count from an agent's per-row data, never its summary headline.** One critic's headline
said 27 PASSED where its own 33 rows said 22. The headline is written last, from memory, and drifts
optimistic.

---

## 2026-08-17 — the machine changed, and one lane replaced two

The work moved from an x86 desktop to a **Raspberry Pi 5** (aarch64, 4 cores, 7.7 GB RAM, no
monitor). `docs/linux-rewrite/ENVIRONMENT.md`'s top section is the measured record. Three
consequences reshape everything below:

**1. `DISPLAY=:1` does not exist here.** XWayland on V3D fails at swapchain creation. The
single-holder X drive lock — described below as "the project's throughput ceiling" — is simply
gone, and so is the lane the `F-BRW` rows depended on. **There is now one lane, and it has no
lock:**

```bash
Scripts/pi-session.sh status      # the PARENT compositor. Nothing renders without it.
eval "$(Scripts/pi-session.sh env)"
export TILLER_WL_LABEL=mine TILLER_WL_BIN=/tmp/mine-tiller
cp rust/target/debug/tiller "$TILLER_WL_BIN"
Scripts/wayland-drive.sh /tmp/shots '<actions>' 20
```

`Scripts/wayland-drive.sh` works here unmodified and carries the full gesture vocabulary. Use
`settle` 15-30; the default 6 was tuned on the faster box. **Never `pkill -x sway`** — it kills the
parent and every other agent's nested compositor. `Scripts/pi-session.sh start` restores the parent.

**2. Two independent throughput ceilings replaced the drive lock.** Workflow fan-out is capped at
`min(16, cores - 2)` — **two agents at a time** on this box. And `main.rs` is 15 751 lines holding
every `TerminalView` construction, so two builders that both need it cannot run concurrently
without one seeing the other's half-finished edits through the shared `rust/target`. The wave shape
that works: **two builders on disjoint crates → one integration build → two fresh critics.**

**3. Only `claude` is installed.** codex, opencode, pi and oh-my-pi are absent, so rows needing
them are environment-blocked here for a different reason than they were on the old box.
`rust/crates/tiller_ui/tests/fixtures/chat_fixture.py` (via `TILLER_ACP_PROGRAM`) is a legal ACP v1
counterparty for states the installed agent cannot produce — say when you used it, and never use it
to stand in for a gesture.

### The one bucket this machine cannot re-exercise: `F-BRW`

Eight of the nine `F-BRW` rows are `PASSED`, and **every one of those passes was earned on the old
box's `DISPLAY=:1` lane.** The embedded browser needs an X11 window handle; on the Wayland lane its
chrome renders and the page does not (`WAYLAND-LANE.md`), and XWayland here dies at swapchain
creation. So there is no lane on this machine that can put a page in a browser tab.

**Say this out loud before the finish line, not at it.** The goal's bar is a full-app critic
ticking every row live; a critic that meets eight rows it physically cannot drive will either
report a regression that is not one or, worse, tick them from the old box's evidence. Neither is
acceptable. The honest options are: re-earn them on a machine with a working X11 lane, or record
them as carried-over-from-another-host with the date and the host named in the evidence column.
The same question applies, more weakly, to every other row whose pass predates 2026-08-17 — the
code did not change, the platform did.

### OWED: wave O is built and UNJUDGED — do not read its commits as verdicts

Four commits landed after the wave-N verdicts and **no critic has judged any of them.** The
workflow's integration and critic phases never ran; the process hosting them exited first.

    e5d4a355  fix(F-TERM-PTY-06): return focus to the terminal after a file drop
    01ec3915  fix(sidebar): close the identity seam one level down, and stop two states sharing a hex
    a72eb8ef  feat(F-CHAT-22/23/31): fold older turns, and make the diff preview a surface
    <plus a follow-up to 01ec3915 in main.rs's tab_agent_mark>

**Their ledger rows were deliberately left alone.** `F-TERM-PTY-06`, `F-CHAT-22`, `F-CHAT-23` and
`F-CHAT-31` still read `FAILED — defective` with the evidence a critic measured *before* these
commits, because that is the last thing anyone actually verified. Upgrading a row because a builder
says it fixed something is the precise mechanism that produced this project's false passes; a
builder's claim is not a verdict, however good the diff looks.

The next critic pass on these four rows should judge them **from scratch against their clauses**,
not against the commit messages. What the builders were briefed to fix, so it can be checked:

- **F-TERM-PTY-06** — the drop must return focus to the terminal. Needs a discriminator, not a
  screenshot: type a marker, move focus away, drop, type another marker, see where it landed.
- **F-CHAT-22** — a collapsed older-turn row that "re-opens in place". A fold that cannot be
  reopened, or that loses scroll position, fails.
- **F-CHAT-23** — the tool-call location must be a real link opening the right file, checked by
  the tab's *content*, not its title.
- **F-CHAT-31** — the diff preview owes line numbers and selectable text. Both are pixel claims.
- **The sidebar seam** — agent identity must reach the *tab* row live, for a pane identified after
  spawn through Layer B, not only for one Tiller launched itself.

### What moved today

Across 2026-08-17/18: `PASSED 353 → 363 · FAILED — defective 1 → 5 · UNREACHABLE 28 → 14`, six
rows wired and judged live. Run `Scripts/ledger-totals.py` rather than trusting this line.

- **Stale `UNREACHABLE`s cost exactly what stale `FAILED`s cost.** Eight rows were parked on
  2026-08-14/15 with reasons of the form "the harness has no primitive for this". The primitives
  were built afterwards and nobody re-ran them. Four were finished work the ledger was denying;
  four were genuine port gaps the stale reason was hiding. **One reason — "a stuck Question waiting
  banner; fs confirms write never ran" — was load-bearing for four rows at once**, and a single
  real click on *Allow Once* retired it: the tool completed and the file on disk changed.
  `EVIDENCE-STANDARD.md` already said verdicts expire. This is the price of not re-running them.
- **"Zero app callers" is the remaining shape of the backlog.** F-CORE-ACT-17/18/22/23 were
  UNREACHABLE for one reason: `tiller_activity` was complete, correct and unit-tested, and nothing
  called it. Half B took no change to `tiller_activity` at all. The same shape still holds
  `F-TERM-02`, `F-TERM-PTY-07/08`, `F-GIT-STATUS-02`, `F-GIT-DIFF-03` and `F-BRW-06`.
- **A repair pass can be judged more harshly than the thing it repaired.** The critic that judged
  the activity wiring passed three rows and then found that *selecting* a worktree changed its
  status — the row reads the window's tab set instead of that worktree's panes, so an error is
  erased the instant you click it and an innocent worktree inherits one. One state, several views:
  the exact problem the piece existed to end.
- **Faults that fail silently are the ones to hunt.** Four lane faults found by driving it, not
  reading it — a `pgrep -x` that could not see a pinned binary because Linux caps `comm` at 15
  characters, a virtual keyboard that expired after ten minutes while `type` kept returning 0, a
  screenshot taken before the window relaid out, a killed parent compositor whose name a leaked
  nested one silently inherited. **Every one reads as "the app ignored my input."**

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

**Do not read a count from this file.** Every snapshot written here has been overtaken within
hours, and this file's own warning — that a paraphrased count already produced a phantom
discrepancy — applies to itself. Run the gate:

```
python3 Scripts/ledger-totals.py          # counts, recomputed from the body
python3 Scripts/ledger-totals.py --write  # after editing rows, to re-sync the totals block
```

Two rules that survive every snapshot, both learned the expensive way:

- **Removing an unearned claim must never manufacture a pass.** Census evidence — a symbol and a
  line number — makes a row `NOT EXERCISED`, never `PASSED`. That substitution is the mechanism
  that produced this project's false passes.
- **Verdicts expire in both directions.** A stale `FAILED` sends a builder to build what already
  exists; a stale `UNREACHABLE` hides both finished work and work still owed. On 2026-08-17 eight
  rows parked as "the harness cannot do this" turned out to be four of each.

`P113-triage.md` groups the `FAILED — defective` bucket by root cause and ranks it by rows-unblocked;
root causes are always far fewer than rows.

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

Reference checkouts (read-only, never copy code) lived at `../\_tiller-refs/{waku,zed,orca,t3code}`
on the old box and **are not present on the Pi**. A critic that wants to check for transplanted code
must therefore judge on internal evidence — foreign identifiers, attribution markers, idioms that do
not match this codebase's own types — or clone a reference itself, read-only. Say which you did.
The Swift original, by contrast, **is** here: `App/` and `Packages/` in this same repo. It is the
authority on what a clause meant, and reading it is not optional when a clause is ambiguous.

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

## Who does what now (2026-08-17) — roles, not panes

The named panes below (`pi`, `sonnet`, `codex12`, `fable`) were herdr panes on the old box and are
gone. The structure that replaced them is the same rule expressed differently, and the rule is the
one thing that must not change:

**The critic is never the agent that built the piece.** Builders and critics are now workflow
subagents spawned per piece. A critic gets the row's clause, the VERIFY line and the lane recipe —
**never the builder's report or reasoning** — and must compile, launch, screenshot and exercise the
thing itself. The orchestrator applies verdicts to `INVENTORY-LEDGER.md`; builders never touch it.

Two mechanics make that honest rather than nominal, and both were added today after they failed:

- **Pin the binary.** A critic and a builder share one `rust/target`. Without
  `TILLER_WL_BIN=/tmp/<label>-tiller` pointing at a snapshot, a builder's rebuild swaps the binary
  under a running drive and the verdict silently describes something the critic never compiled.
- **Commit path-scoped, early.** A builder was killed mid-piece with 1167 uncommitted insertions
  and no report; the next agent had to read `git diff` and judge inherited code it did not write.
  `ENVIRONMENT.md`'s "a shared file is not a reason to leave work uncommitted" is the protocol —
  never `git add -A`, and say in the message when a shared file may carry someone else's edits.

File ownership is still how builders stay out of each other's way, and the two standing rules in
`QUEUE.md` still hold: reassigning a file under a running agent is the orchestrator's bug, and
whoever widens an enum owns every match arm it breaks.


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

# Critic findings log

Findings that were **exercised**, not read. Anything here was reproduced against a running
build; nothing was inferred from source. Each entry names who it was assigned to.

The rule this file enforces: a capability nobody has run does not count as working, and a defect
nobody has reproduced does not count as a defect.

---

## P-001 · ACP works end to end · **PASSED** · verified twice, independently

The hardest test in the brief — connect a real agent, send messages, watch a streamed reply —
**passes on Linux.** The critic exercised it; the orchestrator then verified it without trusting
the critic's word or the app's own UI.

What the transcript showed (`reference/linux-progress/fresh-session.png`):

1. `Write a file named critic-check.txt in the current directory containing exactly the single
   line ---7391` → a tool-use card rendered, marked **Completed**
2. `Permission requested / Answered: Always Allow` → the permission round-trip rendered and resolved
3. `Done — critic-check.txt created with the single line ---7391.`
4. A second turn: `say only the phrase acp-stream-ok-7391 with no other words` → the reply was
   exactly `acp-stream-ok-7391`

The critic used **nonces**, which is what makes this falsifiable rather than merely plausible:
a stub cannot invent `---7391`, and a cached reply cannot contain `acp-stream-ok-7391`.

**Independent verification.** Rather than believe the UI's "Completed" badge, the file was hunted
down on disk:

```
/tmp/tiller-chat-demo-1786578855632471003/critic-check.txt   →   [---7391]
```

Exact match. So the tool call really executed; the badge was telling the truth.

**A false alarm worth recording.** The first search covered only `$HOME` and the two project
directories, found nothing, and pointed straight at a serious conclusion — that the UI reports
tool calls as `Completed` without running them. Widening the search to `/tmp` dissolved it. The
directory name `tiller-chat-demo-<nonce>` appears nowhere in the Rust sources, so the critic had
created it as a scratch area rather than testing inside the repo. Careful of it, and a reminder
that "I could not find the evidence" and "the evidence does not exist" are different claims.

**Ticked:** the ACP core of F-AGENT — agent launch, prompt, streamed response, tool-use
rendering, permission request and answer. Model shown in the composer: `Opus (1M context)`.

**Agent CLIs actually present on this machine:** `claude`, `codex`, `pi`. Absent: `opencode`,
`omp` — inventory entries naming those cannot be exercised here and must be marked unreachable,
not failed.

---

## P-002 · Settings is navigable on Linux · **PASSED** · critic-driven, orchestrator-verified

The critic drove the app with absolute-coordinate `xdotool` clicks and walked into Settings. Frame
`/tmp/f04-agents.png` shows a real, rendered Settings screen: left nav (AI Providers, Agents,
General, Permissions, Appearance) with Appearance selected; a `System | Light | Dark` segmented
control; a Translucency toggle; Interface font size 13pt and Terminal font size 14pt with
steppers; a `SF Symbols | Material` file-icon choice; and per-agent colour swatches for Claude
Code, Codex, OpenCode, Pi and Oh-My-Pi.

Nine frames were captured with six distinct hashes and differing colour counts, which is what
proves the navigation was real rather than nine photographs of the same window.

**A second false alarm, recorded because the pattern matters.** One frame, named `f09-aiprov`,
shows the main window rather than AI Providers, and the first conclusion drawn from it was that
the critic's clicks were not landing and its evidence was worthless. Hashing all nine frames
disproved that immediately. Same failure mode as the `/tmp` file earlier: a single unrepresentative
sample, a confident conclusion, and very nearly an intervention that would have derailed a
correctly-working agent. **Check the whole set before doubting the witness.**

**Consequence for the theme work:** Appearance offers an explicit `System | Light | Dark`
override, so F-002 was never a hard lock-out — but a first launch that lands on Light because no
portal answered is still the wrong default, and pi's fix stands.

---

## P-003 · The terminal accepts input and echoes output · **PASSED** · nonce-verified

Driven by the orchestrator against the live worktree build:

```
./Scripts/linux-drive.sh … "click 800 110; type 'echo ORCH-803525'; key Return; sleep 2"
```

The captured frame (`reference/linux-progress/drive-fixed-test.png`) shows in the terminal pane:

```
>> echo ORCH-803525
ORCH-803525
>>                      ← fresh prompt, 2ms
```

A real shell, a real keystroke path into it, and real output rendered back. The nonce is what
makes it evidence rather than a plausible-looking screenshot.

**This also exposed a defect in my own instrument, which matters more than the tick.**
`linux-drive.sh` shipped with a click helper built on `xdotool --window`, which delivers through
XSendEvent and warps relative to a window id. Neither survives here: the app is an X11 guest of a
Wayland compositor, X focus cannot be forced onto it, and events addressed to the window id are
dropped silently. **Every click and keystroke I gave the builders went nowhere**, and codex11 was
running P4 with it — so any "nothing happened" it observed may have been my tool rather than the
product. Fixed to translate window-relative coordinates to absolute screen coordinates and inject
with plain XTEST, and codex11 was told to re-run anything that rested on a dead click.

The cost of the working route: it moves the operator's physical pointer. There is no other
injection path on this machine, so runs must be kept short.

---

## F-006 · "SF Symbols" is offered as a file-icon set on Linux · OPEN · → unassigned

Visible in the same frame: **Files → File icons** offers `SF Symbols` and `Material`. SF Symbols
is Apple's font and does not exist on Linux; selecting it can only produce missing glyphs. Either
hide the option off macOS or map it to a real fallback. Small, but it is exactly the class of
defect that compiles and renders and is simply wrong here — the same family as F-003's macOS path
and F-004's permissions screen.

Left unassigned on purpose: it belongs to whoever next owns the settings surface, and no builder
should be interrupted for it mid-piece.

---

## F-007 · UPDATE 04:35 — REAL, and it is THE single biggest gap. My 02:45 softening was wrong.

pireview's verdict, after a full pass:

> *The silent crash. **Five observed deaths** across both builds and both displays, roughly every
> 4–13 minutes, no panic, no log, not deterministic — settings-adjacent in 3 of 5. It kills the
> hosted agents and unsaved state; until it is fixed, nothing else in the inventory matters.*

**Five occurrences across two builds and two displays** is not what shared-database contention
between orphaned instances looks like, which is what I proposed at 02:45 after failing to reproduce
it once. One failed reproduction is weak evidence; five sightings under varied conditions is strong
evidence. I wrote "do not spend a builder on this yet" — that instruction is withdrawn.

The pattern that survives: non-deterministic, 4–13 minute intervals, no panic and no log line,
settings-adjacent in most cases. No panic means it is not a Rust `panic!` — look for `process::exit`,
an abort, a signal, or a fatal error inside a GPUI/Vulkan callback that bypasses unwinding. The
absence of a log line is itself evidence: whatever kills it does not go through our error paths.

By the brief's own rule this outranks everything: an app that vanishes has not rendered. It also
silently poisons other evidence, since a capture taken after a death photographs a relaunched app
with restored state, which reads as a UI that "reset itself".

**Lesson for me:** I softened a confirmed finding on the strength of a single failed reproduction,
and told the team not to work on it. Failing to reproduce a defect is not evidence of its absence —
especially for something described from the start as non-deterministic.

---

## F-007 · 02:45 note, retained to show the reasoning that was wrong

Re-run with the repaired driver (the earlier attempt was invalidated by my own broken tool, which
was clicking on windows belonging to other instances). Sequence: gear → General → AI Providers →
Appearance → Agents → Permissions, capturing after each step.

**The app survived every click.** Six frames, distinct colour counts (12928 / 12928 / 12928 / 12945
/ 10806 / 10803), process alive throughout, exit 0.

So the crash the critic saw twice is real but not caused by navigating Settings. A likelier cause,
measured while hunting it:

**Ten instances of the app were running at once**, six of them orphans (`ppid=1`) up to 26 minutes
old, holding ~950 MB between them — leaked by agent runs whose parent scripts exited without
reaping them. Instances launched from the same checkout **share one SQLite session database**, and
contention on that is a plausible route to an exit with no panic and no log line, which is exactly
the signature reported.

Six orphans reaped; the four with live parents were left alone. If the silent exit does not recur
now, contention was the cause and the defect is in lifecycle hygiene rather than in the settings
surface. If it does recur on a quiet machine, it is real and goes back to the top of the queue.

**Do not spend a builder on this yet.** Ask the critic to note whether it recurs now that the
machine is quiet. This is the second time tonight that an apparent product defect turned out to be
an artefact of several agents sharing one display and one database — the first was "input does not
reach the terminal", which was my own tool driving somebody else's window.

Original finding, kept because it may still be real:

---

Found by pireview while exercising F-SET. The process exited **twice**, reproducibly enough to be
described: once after toggling things in General and then opening AI Providers, once on clicking
the Theme category after viewing AI Providers.

No panic. No error line. Nothing in the log but the usual radv Vulkan warning. The window simply
stops existing.

By the brief's own rule — *if it does not compile or does not render, that is the gap by
definition* — an app that **vanishes** outranks everything cosmetic. It also silently invalidates
neighbouring evidence: any capture taken after a crash photographs a relaunched app with restored
state, and a critic could easily read that as a UI that "reset itself" rather than one that died.

Unassigned deliberately: all three builders are mid-piece, and this needs whoever next owns the
settings surface with a clear run at it. It is first in the queue.

---

## F-008 · Provider "Available" badges do not mean the CLI exists · CONFIRMED · unassigned

Also from pireview's F-SET pass: providers show **Available** for agent CLIs that are not on this
machine's PATH. The badge apparently reports "a built-in adapter exists for this provider", not
"the binary was found".

Independently corroborated: `opencode` and `omp` are genuinely absent here (only `claude`, `codex`
and `pi` are installed), yet the screen presents them as available. A status line that is right by
accident on the developer's machine and wrong on the user's is worse than no status line — it
converts a missing dependency into a mystery failure later.

---

## P-004 · The status bar shows real usage data · **PASSED**

pireview verified the provider segments are not decorative: Codex reads `67% 5h`, parsed from this
machine's actual `~/.codex/auth.json` credentials, while Claude and OpenCode Go show `—` because
there is no live session for them. So `tiller_usage`'s fetchers genuinely run on Linux. Stale,
logged-out and error states were not separately exercised and remain unticked.

---

## F-001 · The control socket is never created · **FIXED** · codex12 (P3)

Closed. Verified by the orchestrator against a live app, not taken on the builder's word:

```
tillerctl ping              -> pong
tillerctl capabilities      -> system.ping system.capabilities system.identify
                               workspace.list workspace.current notify
tillerctl identify          -> tiller-linux  linux/gpui-waku  /…/tiller-linux  p-…-wt-1
tillerctl list-workspaces   -> wt-0  tiller-linux  rust/gpui-rewrite  /…/tiller        false
                               wt-1  tiller-linux  linux/gpui-waku    /…/tiller-linux  true
tillerctl current-workspace -> tiller-linux  linux/gpui-waku  /…/tiller-linux  p-…-wt-1
```

`list-workspaces` returns both real worktrees with correct branches, paths and the active flag —
so the server is reading genuine state, not echoing a fixture.

**Six methods are served, and only six.** codex12 reported the scope honestly rather than
shipping thirty that answer without doing anything, which is what was asked for. Everything
outside that list stays FAILED, not untested.

**Why this was the highest-leverage item.** It is the only way to drive the app without injecting
clicks at absolute screen coordinates — which works, but moves the operator's physical pointer and
is slow and brittle. It also gives the critic an independent oracle: comparing what the sidebar and
status bar *display* against what `identify` and `list-workspaces` *report* is a much stronger test
than reading the screen alone.

Build note for anyone repeating this: `cargo build -p tiller -p tiller_control`. `tillerctl` is a
binary of the `tiller_control` package, not a package — `-p tillerctl` fails, and failing that way
silently leaves you testing a stale binary. It cost me one wrong measurement.

---

**How it was exercised.** Built and launched `rust/target/debug/tiller` on Xwayland, waited for
the window, then ran the shipped `tillerctl` against it:

```
$ ./rust/target/debug/tillerctl ping
tillerctl: Tiller control socket is disabled or Tiller is not running
(connect: No such file or directory (os error 2))
```

`capabilities`, `identify`, `list-workspaces` and `current-workspace` all fail identically. A
search for socket files while the app was running turned up none — not under `$XDG_RUNTIME_DIR`,
not under the macOS-style path, nowhere. `tillerctl` itself is built and its CLI surface is real,
so the missing half is the server.

**Why it outranks its neighbours.** It is not that the socket is in the wrong place; there is no
socket. And it blocks more than its own entries: the socket is the only way to drive the app
without simulating input on the operator's physical desktop, so it gates the critic's ability to
exercise anything else headlessly. Roughly **43 inventory entries** (F-CTRL, F-AUTO) rest on it.

**Also noted, unresolved:** the Rust `tillerctl` exposes no pane-driving commands — no
`panel.write`, `panel.read` or `panel.wait` — unlike the macOS design recorded in
`02-inventory-packages.md`. Asked codex12 to report whether that is missing or deliberate, and
not to implement it yet.

---

## F-002 · The app starts in the light theme on Linux · **FIXED** · pi (P1)

Every Linux frame captured up to 02:00 rendered light. `03-visual-bar-and-gpui-patterns.md` §C.5
predicted exactly this from source: appearance is resolved through the XDG desktop portal, and
without an answer GPUI defaults to Light. Predicted from source, then confirmed in pixels.

**Verified fixed** in `reference/linux-progress/p1-theme-progress.png` (02:05): the app now opens
dark, unprompted, on this portal-less setup.

---

## F-011 · The sidebar highlights the wrong worktree, and selecting one does nothing · CONFIRMED · **TOP OF QUEUE**

Found by pireview using the UI-versus-socket oracle. This is the first defect that no screenshot
alone could have produced, because the screen looks entirely plausible on its own.

Four independent sources, three of which agree:

| Source | Says |
|---|---|
| Sidebar highlight after launch | `rust/gpui-rewrite` |
| `tillerctl current-workspace` | `linux/gpui-waku` |
| Status bar, right side | `linux/gpui-waku · ~/Scrivania/Progetti/tiller-linux` |
| The terminal's real shell cwd, via `/proc` | `…/tiller-linux` |

So the sidebar highlights the wrong worktree **and hangs the open Chat and Terminal tabs under the
wrong row**. And separately (F-SID-05): clicking a worktree row moves the highlight — measured, the
`srgb(37,37,37)` fill moves — but `current-workspace` keeps reporting the old one. **Sidebar
selection is cosmetic.** It looks like navigation and changes nothing.

Also from the same batch: `+` (add project) opens nothing at all — measured as a 9×9-pixel hover
difference on the first click and a 0-pixel difference on the second. Drag-reorder does not exist
(`sidebar.rs` has zero drag handlers).

Owner: the sidebar is pi's territory, but pi is mid-piece on the chat surface. Queue this next
rather than interrupting; it is the strongest defect on the board.

---

## Method note · The critic retracted its own findings, and that is the best thing that happened tonight

The same batch opens with this, unprompted:

> *IMPORTANT: earlier sidebar findings from the shared :1 display (dead rows, dead filter,
> spontaneous settings) are RETRACTED — multiple instances stack at one position there and
> clicks/imports cross windows.*

It rediscovered the shared-display hazard on its own, realised its earlier sidebar conclusions were
contaminated by it, and **withdrew them in writing** before reporting new ones. Then it rebuilt its
setup to remove the cause: a dedicated Xwayland `:2` with no other agents on it, a fresh database, a
private socket as oracle, the window pinned by XID, and every click confirmed by a pointer-window
check.

Three of those retracted claims — dead rows, dead filter, spontaneous settings — would each have
sent a builder hunting a defect that did not exist. A critic that only ever adds findings is worth
less than one that removes its own.

---

## P-005 · The socket drives panes end to end · **PASSED** · the scale unlock

codex12's P6 landed. Verified by the orchestrator over the raw socket, not from the builder's report:

```
panel.create {"kind":"terminal"}              -> {"ok":true,"result":{"id":"pane-2717449-1"}}
panel.write  {"id":…,"input":"echo PANEIO-82144\n"} -> ok
panel.read   {"id":…}                         -> base64 of the raw terminal stream, which after
                                                 ANSI stripping contains:
                                                   echo PANEIO-82144
                                                   PANEIO-82144      <- the shell's actual output
```

The nonce appears both as the typed command and as the command's output, so this is a real PTY on
the other end, not an echo of what was sent.

**19 methods served:** system.ping/capabilities/identify, workspace.list/current, notify,
panel.create/split/list/write/key/read/wait/focus/close, notification.create/list/clear, session.ref.

**Why this outranks its entry count.** Every inventory entry until now cost a click at absolute
screen coordinates, which hijacks the operator's physical pointer and is slow and brittle. Anything
happening inside a pane can now be scripted and asserted on. This changes the *rate* of verification
rather than the surface area, which is the only thing that makes 388 entries tractable.

**The envelope, recorded because it cost four wrong attempts:** every request needs `id`, `method`
**and** `params`; the write parameter is `input`, not `text`; `panel.read` returns base64 of the raw
stream, so decode then strip ANSI before matching. Socket at
`/run/user/1000/TillerRust/control.sock`.

---

## F-009 · `panel.list` is blind to the app's own panes · CONFIRMED · → codex12 (P6)

Found while proving P-005. With Chat and Terminal open in the running app, `panel.list` returns
`{"panels":"[]"}`. After creating one through the socket it lists exactly that one, tagged
`tab: "control"`. So the automation surface can only see what it created itself — and the panes an
actual user has open, which is most of what a client would want to address, are invisible to it.

## F-010 · `tillerctl` has no verbs for any `panel.*` method · CONFIRMED · → codex12 (P6)

The server speaks all nine panel methods; the shipped CLI speaks none of them, so every consumer
must hand-roll socket JSON and rediscover the envelope. `capabilities` advertising a method that the
shipped tool cannot invoke is halfway back to the "thirty stubs" problem this project deliberately
avoided.

---

## Method note · Measure the pixels, do not describe them

Three times tonight a confident visual reading was wrong, and each would have sent a builder after
a defect that was not there:

1. *"The critic's clicks are not landing"* — from one unrepresentative frame. Hashing all nine
   showed six distinct screens. The critic was navigating correctly.
2. *"The tool call never ran"* — from a search that excluded `/tmp`. The file was there.
3. *"The tab chip is still saturated blue"* — repeated across three separate frames, stated to two
   builders. Then sampled:

   | Element | Measured |
   |---|---|
   | Selected tab chip | `srgb(39,39,39)` |
   | Unselected tab | `srgb(26,26,26)` |
   | Selected sidebar row | `srgb(37,37,37)` |
   | Window background | `srgb(21,21,21)` |
   | **Files/Changes active segment** | **`srgb(59,130,246)`** |

   The chip was a neutral lift all along. The blue my eye kept attributing to it belonged to the
   segmented control a few hundred pixels to the right, and once seen there it coloured everything
   nearby in the reading.

`convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]' info:` costs one command and settles it.
From here, visual findings carry a measured value or they are not filed. This matters more for the
critic than for anyone: "looks too blue" loses an argument with a builder, "srgb(59,130,246) where
the reference uses a 6% neutral lift" ends it.

---

## F-005 · The new dark theme regressed selection colour · **FIXED (mostly)** · pi (P1) + codex12

Sidebar selection and the active tab chip are both neutral lifts now, measured above. What remains
is precisely one element: the **Files/Changes active segment at `srgb(59,130,246)`** — full-strength
blue, in a hue the visual reference never uses, spent on the least interesting fact on screen.
It belongs to pi's P5, which already carries it as item 3.

Original finding, kept for the record:

Seen in the same frame that confirmed F-002, so this is progress, not failure — but it moves
*away* from the bar while the theme moves toward it.

Selected sidebar rows (`rust/gpui-rewrite`, `Terminal`) and the active Files/Changes segment now
render as a **saturated blue block**. Against a near-black ground that is the loudest thing on
screen, and it is loud in a hue the reference does not use at all. waku marks selection with a
barely-there raised fill — a few percent of neutral lift — and spends its one saturated colour
(coral) sparingly, on inline code and links. Ours spends full-strength blue on "which row is
highlighted", which is the least interesting fact on the screen.

Changed-file names in the tree are also amber at full strength, competing with the same budget.

The point is not that blue is wrong. It is that **selection is chrome, and chrome does not get
the accent**. This is the same axis as the density verdict in `CRITIC-visual-baseline.md`: what
is permitted to be visually loud at rest.

Recorded rather than dispatched, because pi is still mid-piece on P1 and may already be on it.
Interrupting a builder with a finding it is currently fixing costs more than it saves.

---

## F-003 · Session state is written to a macOS path on Linux · CONFIRMED · → codex12 (P3)

Observed in the running app's own log:

```
[session] state is checkout-local at
/home/enzopalmisano/Library/Application Support/TillerRust/checkouts/…/tiller.sqlite
```

`~/Library/Application Support` is a macOS location. On Linux this must follow the XDG Base
Directory spec. It does not crash, which is what makes it easy to miss.

---

## F-004 · The Permissions screen cannot exist on Linux as specified · CONTRACT ISSUE

Read from `reference/shots/10-settings-permissions.png`. The whole screen is macOS machinery:
its own header says *"Terminal tools inherit Tiller's **macOS privacy envelope**"*, and it lists
Notifications, Screen Recording, Accessibility, Full Disk Access, Automation and Local Network,
each badged `GRANTED` / `DENIED` / `CHECK MANUALLY`, each with a button that opens macOS System
Settings at the right pane.

Linux has no TCC, no per-app privacy database, and no System Settings deep links. There is
nothing to query and nothing to open. So roughly **8 inventory entries (F-PER)** cannot be ticked
as written — not because they are unbuilt, but because their subject does not exist here.

**Ruling, so nobody quietly fakes it:** on Linux the screen keeps its slot and changes its
content to what actually gates the app on this platform — XDG desktop portal availability (the
same portal that decides dark mode and notifications), and the **Browser origin grants** section,
which is Tiller's own permission model and is fully portable. Anything with no Linux meaning is
removed rather than shown permanently as `CHECK MANUALLY`, which would be a lie in the shape of
a feature.

The macOS entries stay in the inventory and stay tickable on macOS. On Linux they are marked
`N/A — platform`, which is a third state distinct from passed and failed. A critic must never
tick them here, and must never fail them here either.

---

## Note for P1 · Tiller already has a warm accent

From `reference/shots/15-chat-composer-focused.png`: on focus the composer's border turns amber
and the send button becomes a filled amber circle. So the old app's "ready to act" accent is
already warm, and much closer to waku's coral (`#C85F44` / `#E2795B`) than the blue used on the
segmented controls. Moving to waku's palette is therefore less of a reversal than the Files /
Changes toggle suggests — the warm accent has a precedent here worth keeping.

---

## Environment findings (not product defects — recorded so nobody re-derives them)

**Only the real Xwayland display can present.** Vulkan needs DRI3 to present on X11. Measured:

| Display | DRI3 | App renders |
|---|---|---|
| Xvfb | no | window maps, every frame black |
| Xephyr `-glamor` | no | window maps, every frame black |
| Xwayland `:1` (real session) | yes | renders, ~11.4k distinct colours |

So screenshots must be taken on `:1`. A capture that comes back with one distinct colour means
presentation failed, not that the UI is empty — `Scripts/linux-shot.sh` now fails on that rather
than reporting success.

**Synthetic input is not usable yet.** On `:1` the window is a guest of the Wayland compositor:
`xdotool windowactivate` cannot move X focus to it, so injected keystrokes land elsewhere, and
clicking would have to hijack the operator's physical pointer. This is precisely why F-001
matters — the control socket is the supported way to drive the app, and it is the one that
should be fixed rather than worked around.

---

## Appended 09:10 — one correction, one landed piece, one new finding

### Correction — synthetic input IS usable (supersedes the closing paragraph above)

The section above ends "Synthetic input is not usable yet", and that is no longer true. It fails
only the way it was first attempted. `xdotool --window <id>` delivers through XSendEvent, which
this app drops because it is an X11 guest of a Wayland compositor and X focus cannot be forced onto
it. Aiming at **absolute screen coordinates** and letting plain **XTEST** deliver the event does
land, and `Scripts/linux-drive.sh` now does exactly that.

The cost is real and is why the original note was written: it moves the operator's physical
pointer. Keep runs short. The control socket is still the better instrument where it reaches.

The reason this correction matters beyond the fact itself: a whole run was once written off as
"input does not work" when the input tool was broken, not the product. **Before believing a
negative result about the app, check the instrument.** That has now caused three false negatives.

### Landed — P11, settings persistence (codex12), with the evidence it needed

Appearance → Dark, kill the process, restart, Dark restored; the SQLite row `appearance.theme=dark`;
screenshot at `reference/linux-progress/p11-settings-restored.png`; `tiller_ui` 32 tests and
`tiller` 25 tests passing. This closes the critic's "settings toggles don't persist".

### F-009 — the worktree-selection arm satisfies the compiler and does nothing

`SidebarEvent::SelectWorktree` is emitted on click (`tiller_ui/src/sidebar.rs:1087`) and handled at
`tiller/src/main.rs:844`. The handler's entire body hands the path back to the sidebar via
`set_selected_worktree`. The round trip ends where it started.

Verified by reading the state, not inferred: `ControlState::current` (`main.rs:102`) is an
`Option<usize>` index, assigned **exactly once** at `main.rs:123` from
`workspaces.iter().position(|w| w.selected)` when state is built from persistence, and **never
written again**. So `workspace.current` over the control socket keeps answering with the worktree
selected at startup no matter what the user clicks. The status bar's branch/path and the tabs'
home worktree are stale for the same reason.

So the critic's original finding — *sidebar worktree selection is cosmetic; socket and shell
disagree with the highlight* — **still stands unchanged**, and now has an extra hop in front of it.

**The lesson, which is the reason this is written down.** The handoff between the two agents was
deliberately made *compiler-enforced*: adding the enum variant made the shell's match
non-exhaustive, so the tree would not build until an arm existed. That was a good mechanism and it
did its job — it guaranteed the arm exists. It cannot check that the arm does anything, and this
one does one of the four things the handoff specified. A green build is evidence about types, not
about behaviour. **The only thing that can catch this is exercising the feature** — which is
precisely what the critic's rule already says, and precisely what nobody had done here yet.

Routed to codex12 as P9b with a live acceptance test: `tillerctl` current-workspace before the
click, after the click, and a screenshot of the highlight — all three agreeing, pasted verbatim.

---

## Appended 09:30 — P14 first attempt: a good instrument, an invalid trial

codex11 built `Scripts/crash-supervise.py` and proved it by self-test: a clean exit is captured as
`exit_code: 7`, and `kill -SEGV $$` is captured as `WIFSIGNALED: True`, `WTERMSIG: 11`,
`signal_name: SIGSEGV`, `core_dumped: True`. The instrument is real and it stays — decomposing wait
status is the fact that splits this search space more than any other, because `SIGSEGV`, `SIGABRT`,
`SIGBUS` and `SIGKILL` look identical from outside and mean four different things.

It then ran three concurrent instances for 300s and again for 480s and saw **no crash** — about 39
minutes of process time against a defect with a 4–13 minute mean time between deaths. That reads
like strong evidence of absence. It is not, and the reports say why in their own output:

```
display: :102
screenshot: …-supervised.png (flat: 1 colors; presentation may be blank)
```

**All six runs captured a flat one-colour frame: the app never presented.** Vulkan needs DRI3 to
present on X11, and a one-colour capture is the measured signal that presentation failed. So every
supervised process was a healthy event loop drawing nothing, with nobody operating it — while
every observed death happened with the app live on screen, three of five while navigating Settings.

Also: the run fell back to `:102` reporting that `:1` was unavailable. `:1` answered `xdpyinfo`
immediately afterwards.

**The trial could not have produced a positive result, so its zero carries almost no information.**

This is the third time this project has nearly closed a question on a negative produced by a broken
instrument or unrepresentative conditions — after the unpaced poll loop that declared "never
rendered" in one second, and the `xdotool --window` targeting that declared "input does not work".
The rule earned here, and now written into the harness rather than only into the notes:

> **A run in which the app never rendered is not a clean run. It is not a trial at all**, and it
> must never be added to a "no crash in N minutes" total.

Routed back to codex11 as P14b: classify runs VALID TRIAL / NOT A TRIAL in the supervisor itself,
run on `:1` where the app actually presents, drive the app throughout — heaviest on Settings — and
report **valid trial minutes** separately from wall-clock.

---

## Appended 09:45 — F-009 closed, and a likely chain reaction behind three other findings

### F-009 closed (codex12, P9b) — with the live proof it was asked for

```
BEFORE  tiller-linux  linux/gpui-waku   /home/…/tiller-linux   p-c6ce0efaea1b05ec-wt-1
AFTER   tiller-linux  rust/gpui-rewrite /home/…/tiller         p-c6ce0efaea1b05ec-wt-0
OK  selection-after.png (1470x833 · 9303 colours · window 0x200001)
```

9303 distinct colours matters as much as the text: it is the measured proof that this frame is a
real rendered UI and not the blank rectangle that a failed presentation returns.

`ControlState::select_worktree` (`main.rs:145`) now writes `current` and re-derives every row's
`selected` flag; the shell-level `select_worktree` (`main.rs:1104`) re-homes tabs and refreshes the
status bar, then confirms the sidebar. Persistence rides on the existing `schedule_save`, and a
restart restored the selection. 27 `tiller` tests, 29 control integration tests.

### The chain reaction worth noticing

Three of the critic's socket findings — `panel.list` cannot see app panes, `panel.wait` ignores its
timeout, user-mode `notify` missing — were recorded at 02:55. Reading the code now, at least two
look false:

- `panel.wait` (`main.rs:429`) parses `timeoutMs`, rejects non-integers with a clear error, and
  passes `Some(Duration)` to `PaneRegistry::wait`, which uses `wait_timeout`.
- `main.rs:1084` registers the app's own panes through `set_external`, so `panel.list` ought to see
  them.

And there is a specific, testable reason `panel.list` would have *appeared* blind: it resolves its
directory through `state().working_directory(worktree)`, which with no `worktree` parameter falls
back to `current_workspace()` — **the very value that was frozen at startup until P9b landed**. A
stale current workspace means `list_for` filters panes against the wrong directory and correctly
returns none.

If that holds, the critic observed something real and attributed it to the wrong method: one
underlying defect wearing three faces. It is a good reminder that **findings are hypotheses with
timestamps**, and that a fix landing anywhere can silently retire findings filed elsewhere.

Routed to codex12 as P17 with the rule that each of the three comes back **STALE** (a live
transcript showing it works, plus why it looked broken) or **REAL** (failure, fix, fixed
transcript) — and that "fixing" a method which already works is not free, because the critic is
depending on it right now.

---

## Appended 09:50 — P17: two findings retracted with evidence, one real and fixed

codex12 re-exercised the three socket findings rather than repairing them on the strength of the
report, and the hypothesis in the 09:45 entry held:

- **`panel.list` — STALE.** Live output lists the app's own panes (`pane-0`, `pane-1`). The stale
  workspace selection was the cause of the original false failure, exactly as predicted: the method
  resolved its directory through `current_workspace()`, which was frozen at startup until P9b.
- **`panel.wait` — STALE.** A 180 ms timeout returned in 0.18 s, and a separate pane returned exit
  code 7 — so it honours the timeout *and* distinguishes "timed out" from "exited with code N",
  which is the distinction that actually matters. A wait that cannot tell them apart reports a lie
  quickly, which is worse than hanging.
- **`notify` — REAL.** A raw title/body notification returned "notify requires session". Fixed at
  `main.rs:223` with a regression test; it now returns `ok:true` and the notification is listed.

**One underlying defect had been wearing three faces.** Two accusations are retracted, one bug is
closed, and — the part worth keeping — two methods the critic is currently depending on were *not*
rewritten on the strength of a stale report. "Fixing" working code is not free.

### The automation surface, measured

All nine entries of `## Control socket and automation surface` exercised; **four pass**: F-AUTO-02
(create a panel), 03 (split/list/write/key/read/wait/focus/close), 06 (notifications
create/list/clear), 07 (ping/identify/capabilities). 28 `tiller` tests, 29 `tiller_control` tests.

Five real gaps remain, now routed as P18: `worktree.set` (F-AUTO-04); workspace
select/create/close (F-AUTO-05); `session.restore` (F-AUTO-08); `browser.*` answering with explicit
unsupported errors rather than silence (F-AUTO-09); and the socket enable/disable setting with its
path displayed (F-AUTO-01 — the settings row lives in pi's file, so that half must be routed).

The constraint written into P18, and the reason it is written down: **`workspace.select` must call
the same `select_worktree` the sidebar click calls.** A second route into the same state that does
not maintain it is precisely how F-009 happened — a view saying one thing, a socket saying another,
and no way to tell which is lying. One source of truth, many doors into it.

### Note for anyone running the gate

`Scripts/ci.sh` shells out to `xcodegen` and **cannot run on Linux**. Its absence is not a failure
of the work; on this platform the gate is `cargo test` plus live transcripts.

---

## Appended 10:05 — the blank-frame mystery, solved, and a tool that was guaranteeing it

Three agents independently reported that captures had gone blank. That is a signal, not three
coincidences, so it was worth the orchestrator's own time rather than another builder's.

**Cause, from the compositor's own journal:**

```
cosmic-comp: Failed to render texture … import for wrong devices DrmNode { dev: 57984, ty: Render }?
             … modifier: Unrecognized(144115188076389125)
```

`57984` is `renderD128`, the only render node. `144115188076389125` is `0x0200000000000005` — top
byte `0x02`, the **AMD** vendor code — a tiling modifier cosmic-comp's renderer does not recognise
and so cannot import. A Mesa/compositor mismatch entirely outside this project.

**What made it actionable:** Xwayland negotiates its dmabuf modifier set **once, at startup**, and
keeps it. Displays that existed before the mismatch still present; every display created afterwards
never will. That single fact reconciles every confusing observation:

| Tried | Result |
|---|---|
| three consecutive fresh Xwaylands | 1 colour each |
| Xvfb | 1 colour |
| Xvfb + software Vulkan (lavapipe) | 1 colour |
| native Wayland, no X | app runs; `grim` cannot capture — cosmic-comp has no `wlr-screencopy` |
| `:1`, alive since the session began | **8787–8916 colours at 1440x833** |
| `:2`, the critic's, alive for hours | 7729–8615 colours at 1920x1080 |

`:1` was at 640x480 and was enlarged with `xrandr --output XWAYLAND0 --mode 1440x900`; it still
presented afterwards, so resizing does not lose the good modifier set.

### The part that is our fault

`Scripts/linux-shot.sh` created its own Xwayland whenever no display was named — originally for
isolation, and a good idea when it was written. After the mismatch appeared, that default
**guaranteed** a blank frame: the tool's most convenient invocation was the one that could not
possibly work. It has been changed to default to `:1`, with the private-server path kept behind
`TILLER_NEW_DISPLAY=1` and a warning that says exactly what will happen.

Note what this cost before it was found: codex11 ran six supervised crash trials against a program
that never drew a pixel, and pi could not re-capture a finished screenshot. Neither was a mistake by
either agent. **The default of a shared tool is a decision that gets made once and then silently
applied hundreds of times** — when the environment moves under it, it stops being a convenience and
becomes a trap.

That the flatness guard existed at all is why this was diagnosable: the tool refused to call a
one-colour frame a pass, every time, for hours. A capture tool that silently returned black would
have had this project ticking "renders correctly" against a blank rectangle.

**Recipe now in `STATE.md`:** builders use `:1`, the critic keeps `:2`, nobody creates a fresh
display. Sharing `:1` is safe only because both scripts match `_NET_WM_PID` — the filter added after
two agents photographed each other's builds.

---

## Appended 10:35 — the crash is not a crash, and a pattern that has now cost three pieces

### F-007 REFRAMED — it is a presentation freeze, not a process death

The critic's pass-2 headline, and it is the most valuable finding of the day:

> Window pixmap frozen (0 px / 2 s), **process and control socket alive**, recurring at 4–13 min.
> Hunt the frame loop, not the process.

This explains, at one stroke, every property that made the defect so hard to attack: no panic, no
log line, no exit status, and no reproduction under a supervisor watching for a process to die.
codex11's six supervised trials found nothing **because nothing was dying**. That non-reproduction
was a correct measurement of the wrong quantity.

Worth keeping for the method: the earlier note said a run where the app never rendered is not a
trial. True, and insufficient — the harness also has to be watching the right thing. A supervisor
that decomposes wait status is exactly the right instrument for a process death and exactly the
wrong one for a render loop that stops while the process stays healthy. **Before believing a
negative, check that the instrument measures the quantity in question**, not merely that it works.

Routed to codex11 next. The critic keeps observing it — socket responsiveness during a freeze,
whether state still changes behind frozen pixels, whether it ever recovers — without going into the
render code, because its independence is what makes its verdicts worth anything.

### F-010 — the Changes surface is not mounted (the named biggest gap)

Confirmed independently:

```
$ grep -rn "ChangesTab" crates/ --include=*.rs | grep -v crates/tiller_ui/src/changes.rs
crates/tiller_ui/src/bin/changes_preview.rs:7 …
crates/tiller_ui/src/bin/changes_preview.rs:26 …
```

**The only thing in this repository that mounts `ChangesTab` is a demo binary.** The right panel is
Files and Activity; there is no Changes anywhere in the app. Meanwhile the component is ~900 lines
that fully work — three sections, a file correctly in two at once, per-file counts, collapsed bands,
stage/unstage/discard, 38 passing tests. F-CHG-07..18 all fail for this one reason; roughly 20
inventory entries hang on it.

### The pattern, named because it is now three for three

| Piece | Built | Tested | Reachable by a user |
|---|---|---|---|
| `SelectWorktree` arm (F-009) | yes | build green | **no** — it handed the event back to the sidebar |
| Changes surface (F-010) | yes | 38 tests | **no** — mounted only by a demo binary |

**Green tests prove a component behaves; only mounting proves a user can reach it.** A crate that
compiles and a binary that does the job are different claims, and this project keeps paying to
learn the difference. P21 therefore ends with a shell-level guard: a test that fails if the
application can no longer construct and mount a Changes surface — the assertion that would have
caught this the day it happened.

### Two findings retracted — and the snapshot trade-off that caused them

The critic also reported the sidebar still cosmetic, and `session.ref` / `workspace.*` /
`session.restore` unimplemented. Both were **already fixed** before its observation window
(09:09–09:20); the live tree had moved at 09:43 and again at 10:20.

This one is the orchestrator's fault. The critic was told to `cp -a` the tree so that a mid-edit
build failure from three concurrent builders would not be misread as a product defect. That is
sound, and **nobody stated its cost: a snapshot buys stability and pays in currency.** Pass 3 now
requires the snapshot time in the report, and a check of whether the live tree has moved under a
finding before it is filed.

### P18 closed (codex12), with transcripts

`worktree.set` + `session.ref` + `notify` producing a real identity association; workspace
list/current/select/create/close with UI selection confirmed by screenshot; `session.restore`
restoring the worktree and its Chat/Terminal panes; every `browser.*` returning an explicit
Linux-unsupported error and none advertised in capabilities; socket enable/disable exposing
`socketEnabled` and `socketPath`. 70 tests, rustfmt and diff checks clean. It also closed its `:2`
app instance and removed its stale socket — on a shared display that is what keeps other agents'
evidence trustworthy.

---

## Appended 10:50 — two findings verified away, and the pattern named for the fourth time

### Verified NOT defects (checked before assigning anyone to fix them)

- **"The black strips at the top and left of the frame."** pi flagged them as a pre-existing
  transparent-titlebar problem and correctly left them alone. They are a **capture artefact**: an
  independent capture of the same app on `:2` fills the frame edge to edge with no black margin. A
  root crop taken at the wrong origin produces exactly that picture. Nothing to fix — and this is
  the second time a root-crop frame has produced a phantom defect, which is why
  `import -window <id>` is the preferred path.
- **"SF Symbols is still offered on Linux."** The frame showing it predates the fix. The code is
  correct: `#[cfg(target_os = "macos")]` on `SEGMENTED_FILE_ICONS`, an
  `available_on_this_platform()` predicate, and `clamp_file_icons` correcting a value persisted on
  another platform — which the Swift-parity database will hand it, since that default is
  `SfSymbols`.
- **"The `+` new-tab button doesn't render."** A live capture at 10:25 shows `+` affordances both in
  the Projects header and at the right end of the tab strip. Filed from the 09:09 snapshot; stale.

Three accusations retired by looking, at a cost of about two minutes each. All three would have
consumed a builder-piece.

### F-008 — the provider badges are hard-coded, and the truth already exists in a crate

`crates/tiller_ui/src/settings.rs:785–809` holds **five literal `"Available"` strings**. Meanwhile
`crates/tiller_agents` already exposes, and already tests:

```rust
pub fn discover_availability() -> Vec<AgentAvailability>
pub fn find_executable_on_path(program: &str) -> Option<PathBuf>
impl AgentAvailability { fn is_available(&self) -> bool; fn status_label(&self) -> &'static str }
```

On this machine `claude`, `codex` and `pi` are installed and **`opencode` and `omp` are not**, so
two of those five badges are false as displayed.

### The characteristic failure of this codebase, stated plainly

Four instances, all found today, all of the same shape:

| # | The crate does the right thing | The surface does something simpler and wrong |
|---|---|---|
| F-009 | `select_worktree` maintains the control state | the event handler re-highlighted and stopped |
| F-010 | `ChangesTab`, ~900 lines, 38 tests | mounted by nothing but a demo binary |
| P16 | a runtime-resolved mono font token | five call sites naming a macOS font |
| F-008 | `discover_availability()` reads `PATH` | five hard-coded `"Available"` literals |

**Tested crate plus untruthful surface.** Every instance passes every unit test, every time —
because the tests test the crate, and the defect is in what the surface chose to do instead. Unit
tests cannot see this class at all; only mounting the thing and looking at it can.

The question that catches it, and the one now written into the briefs: **can a user reach this, and
does it show what the crate actually computed?**

---

## Appended 11:00 — F-010 half-closed, the display stack pronounced dead, and honest NOT EXERCISED

### F-010 — the Changes surface is mounted (codex12, P21)

`ChangesTab` is now a first-class Diff tab: opened from `+ → Changes`, bound to the selected
worktree and following it, persisting and restoring with the other tabs. 42 tests, **including the
shell mount guard** — an assertion at the application level that fails if the surface stops being
reachable. That guard is the durable part: it is the check that would have caught this on the day it
was introduced, and it is the first structural defence this project has against its characteristic
failure.

Reported honestly and left undone: **the screenshot was not produced and live stage/discard was not
exercised**, because no display exists to do it on. Those remain owed. A plausible screenshot would
have been worth less than that sentence.

### The display stack is gone, and it is not ours

Established by experiment rather than inference:

- `:1` died at 09:56.
- `:2`'s Xwayland process is **alive and sitting in `ep_poll`**, yet `xdpyinfo` from a fresh,
  unrelated client times out.
- The app instance on `:2` was killed to test the obvious hypothesis that a client held an
  `XGrabServer`. **The display did not recover.** So nothing outside is wedging it.
- Fresh displays do not present at all, under any driver combination tried: radv, software Vulkan
  via `VK_DRIVER_FILES=…/lvp_icd.json`, `MESA_VK_WSI_DEBUG=sw`, Xvfb, and native Wayland (where
  `grim` cannot capture because cosmic-comp implements no `wlr-screencopy`).

**F-007 reframed a second time.** The app's own thread state during a freeze — main in
`poll_schedule_timeout`, twelve workers in `futex_do_wait`, timer in `ep_poll` — is a program
waiting politely for a frame callback, not one spinning or stuck in a GPU ioctl. And **no bug in
Tiller's frame loop can make `xdpyinfo` time out for an unrelated client.**

Recorded as a **hypothesis with its status attached, not a verdict**: there may also be a genuine
freeze in the app, and that cannot be tested until the environment is healthy. F-007 was softened
once before on weaker evidence and that was wrong; the lesson is not "it was environmental after
all", it is *say what the evidence supports and say what it does not settle*.

### The headless route — verified, and better than the display ever was

```bash
export TILLER_SOCKET=/tmp/<name>.sock
env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller &
rust/target/debug/tillerctl current-workspace     # real state
```

The app runs with no `DISPLAY` and no `WAYLAND_DISPLAY`, stays alive, and serves the control socket.
`TILLER_SOCKET` isolates the endpoint, so **every agent runs its own instance concurrently with no
contention** — strictly better than four agents sharing one display and occluding each other.

Exercisable: every socket method, state transitions, persistence across restarts,
worktree/project/tab management, and terminals — PTYs need no display, and a nonce proves a round
trip. Blocked: anything that must be *seen*.

**The discipline that goes with it:** anything needing pixels is `NOT EXERCISED — blocked on
display`. Never approximated, never ticked from reading the code. An inventory whose ticks came
from source-reading would be worth nothing, and this is exactly the pressure under which that
starts to look reasonable.

---

## Appended 11:25 — the command layer proven without a display, and two real persistence defects

### P25 — a keyboard layer that cannot be typed into, proven anyway

All pane and tab actions are wired to `TillerWorkspace`'s root handlers, and the same transitions
are reachable as `pane.split`, `pane.focus`, `pane.close`, `tab.cycle`, `tab.select`. The headless
transcript shows the state actually moving: split `pane-1 → pane-2`; focus left `pane-2 → pane-1`;
close removes `pane-1`; tab select activates `pane-0`. 81 tests.

The design constraint is what made this possible: **the action and the socket method call the same
function.** Not two implementations that agree — one function with two doors. Because of that, a
command layer nobody can type into today is still verified today, and the only thing left unproven
is the narrow, honestly-stated remainder: *the chords themselves reach the actions.*

That constraint came directly from F-009, where two routes into one piece of state, only one of
which maintained it, produced a view and a socket that disagreed with no way to tell which lied.

### `Scripts/ci-linux.sh` exists and prints `CI OK`

`Scripts/ci.sh` shells out to `xcodegen` and gates the Swift app; the Rust workspace had **no gate
at all** while three agents edited it continuously. The new one runs fmt, clippy, the workspace
tests, the two Python suites, and a **headless smoke test** — start the app with no display, prove
the socket answers, and round-trip a generated nonce through `panel.create`/`write`/`read`. Its
header states what it does not cover: anything visual.

### F-PER-01 FAILED — the tab came back and the terminal was empty

> tabs/worktree restored, but terminal scrollback was fresh after relaunch

The entry lists scrollback alongside tabs and chats, and its `VERIFY` clause says to produce
terminal output, quit, relaunch, and confirm it returns.

**This is the codebase's characteristic failure in a new place.** The tab persisted, so every
structural check passed — while the thing the user cares about was gone. *Persisting a handle is
not persisting a surface.* The same sentence covers the sidebar event that re-highlighted without
moving state, the Changes surface that had 38 tests and no mount, and the badges that report a
catalogue instead of a `PATH`.

### F-PER-03 FAILED — three panes in, two panes out

A layout that silently drops a pane is worse than one that does not persist at all: the user cannot
tell that they lost one, or which. Whether the tree serialises incompletely or restores incompletely
is answerable by dumping what was written and comparing — two different fixes, and the evidence
distinguishes them.

Both routed back to codex12 as P28. Note that codex12 **found both defects in its own area and
reported them as FAILED** rather than softening them. A finding an agent produces about its own work
is worth more than one extracted from it later, and it should cost nothing to volunteer.

---

## Appended 11:50 — a tool defect of mine that cost the critic 90 minutes

The critic had made **no progress for over an hour**: 0 of 6 todos, context barely moving. It was
not stuck thinking. Two of its processes had been blocked since 10:17 and 10:29:

```
2435977  01:26:02  S  import -window 0x200001 /tmp/critic-pass3/shots/w-0x200001.png
2479932  01:14:10  S  xdpyinfo
```

**A wedged X server accepts the connection and then never answers.** Xwayland was alive and sitting
in `ep_poll`; every X client that touched it blocked forever, with no timeout of its own and no
output to say why. The instruction I broadcast at 10:32 telling everyone to work headless never
reached the critic, because it was already inside `import` when it arrived.

Both processes were killed. **The display then answered again** — so a stuck client had been
wedging the server after all, and `linux-shot.sh` now completes on `:2` in 5.5 seconds instead of
hanging. It still reports one distinct colour: responsive, and presenting nothing.

### The fix, and the general rule

Every X client in `linux-shot.sh` and `linux-drive.sh` now runs under `timeout 10` — `xdpyinfo`,
`xwininfo`, `xrandr`, `xdotool`, and `import` (which already had one). The reasoning is in the
script header so it survives the next rewrite:

> **A hang is the worst failure mode a tool can have.** Unlike an error it produces no evidence and
> no deadline, and from outside it looks exactly like an agent thinking hard. Anything that talks to
> a server it does not control gets a deadline.

This is the fourth tool defect of mine to cost an agent real work — after the unpaced poll loop that
declared "never rendered" in one second, the `xdotool --window` targeting that delivered nothing,
and the `linux-shot.sh` default that created a display which could not present. Every one of them
produced a *plausible* wrong answer rather than an obvious failure, which is the property that makes
tool defects so expensive: the agent has no reason to doubt the instrument, so it doubts its own
work instead.

The corresponding orchestrator duty: **when an agent has been "working" far longer than the piece
should take, look at its processes before assuming it is thinking.**

---

## Appended 12:10 — widening what can be verified, without widening what "verified" means

Three pieces landed while no display existed, and they share a shape worth naming: **when the
evidence you need is unreachable, make the state observable rather than lowering the standard.**

- **P30 (codex11)** — `TerminalStateSnapshot` plus scrollback capture and replay in
  `tiller_terminal`, bounded by the `SCROLLBACK_LIMIT` codex12 had already defined, with
  `session.rs` untouched. Then the split that matters: of 8 display-blocked `F-TERM` entries,
  **7 are state-backed and exactly 1 is genuinely pixel-only.** Holding that line is what makes the
  widening trustworthy — calling all 8 state-backed would have raised the number and lowered its
  meaning.
- **P28 (codex12)** — F-PER-03 fixed by replaying a recorded `PaneEvent` history rather than
  serialising the tree shape, which keeps working when the tree gains a node type.
  `pane-0, pane-1, pane-2` now all survive a relaunch. F-PER-01's persistence half done and the
  renderer-owned capture seam **routed instead of reached into** — landing in codex11's hands while
  it was already editing that crate.
- **P23 (pi)** — provider badges wired to `discover_availability()`, the socket row with its
  resolved path, Permissions gated off non-macOS. It also **declared a five-line edit to another
  agent's file** and **volunteered a defect nobody had counted**: the AI Providers cards still
  hard-code `"Active"` account statuses.

### The discipline this depends on

Every one of these could have been a quiet redefinition of "works". The rules that stopped it:

- An entry about a user *seeing* something is **half-proven** by a test that the value exists, and
  must be recorded in those words — not as a pass.
- A socket method must report **what the surface actually holds**, never recompute an answer of its
  own. A socket that computes its own answer can agree with `git status` while the user's screen
  shows something else — which is this codebase's characteristic failure, now seen five times.
- Conformance tests against the frozen waku measurements prove conformance to the bar, **not that
  the result looks right**, and the test module has to say so, or a green suite gets read as "the UI
  matches waku".

### Still owed, and stated plainly

The live stage/discard through the Changes UI. The screenshot of Settings showing two of five
providers unavailable. The visual comparison against waku's frames. F-TERM-02. Every entry marked
`NOT EXERCISED — blocked on display`. None of these are approximations waiting to be upgraded; they
are debts, and they stay debts until a display exists.

---

## Appended 12:15 — critic pass 3: the freeze settled, and a whole feature area found missing

### F-007 settled, by the right measurement and by someone who did not form the hypothesis

The critic's headless probe: **ping ponged, state changes applied, RSS flat at 15:55 elapsed** —
comfortably past the 4–13 minute window in which the app had "frozen" five times. And its 10:18
boot frame was one distinct colour **from frame one**, not after some interval.

Process and state healthy; the failure is confined to the presentation layer. That is independent
corroboration of the compositor-side reading, and it is worth more than the reading was, because
the critic did not form it and had every reason to look for an app defect instead.

F-007 is therefore recorded as **environmental, with the app side exonerated by measurement** — not
by argument, and not by the failure to reproduce it that was nearly accepted as proof at 02:45.

### F-011 — there is no editor

`FileView` is a **read-only viewer**: no text editing, no save, no dirty state, no dedupe of a file
already open. **8 `F-EDIT` entries fail by structural absence rather than by defect**, and the
critic drew that distinction explicitly, which is the useful part: an absent feature and a broken
one need different work and different estimates.

This is now the largest missing feature area in the project. Routed to pi, which owns
`tiller_ui/src/file_view.rs`.

### F-012 — projects duplicate themselves, and the mechanism is understood

Three project rows listing the same worktree set, visible in a 10:25 capture and **dismissed at the
time as correct git behaviour** — any checkout of a repo does list all its worktrees. That was half
the story. The critic found the rest:

- `restore` re-discovers git worktrees under **every** project row, and **a linked worktree's root
  is itself a repo**, so each discovered worktree becomes a project that discovers the others.
- `write_catalog` runs on create and close but **never on quit**, and writes **two id conventions
  into one table**.

Worth noting as a method point: a plausible explanation ("that is just how git works") retired a
real finding for two hours. The tell was there — the row count kept growing — and nobody counted.
Routed to codex12.

### Pass-3 counts

12 PASSED · 1 FAILED (`session.ref` is in-memory only and does not survive) · 0 UNREACHABLE, plus
the 8 `F-EDIT` absences and the duplication finding. The **entire visual tier** — `F-EDIT-09…12`,
the `F-TAB` leftovers, `F-CHG-03…08`, `F-WIN`, `F-AUTO-01`, the newly mounted Changes surface, and
the freeze's pixels — is `NOT EXERCISED — blocked on display`.

In the critic's own words: *pass-3 verdicts are socket-deep by design, not by choice.*

---

## Appended 12:25 — the last two unreachable surfaces opened, and both proved by disagreement-testing

### P31 (codex12) — `surface.changes.*`, `surface.settings.*`, `panel.state`, `panel.scrollback`

All advertised in `system.capabilities` and exposed in `tillerctl`. 35 control integration tests,
59 UI tests, `CI OK`.

Proved the only way that means anything — **two independent sources agreeing**:

| Surface | The app said | The world said |
|---|---|---|
| Changes | staged 1, changed 1, untracked 1, each `+1 -0` | `git status --porcelain`, same |
| Settings | `claude`, `codex`, `pi` available; `opencode`, `omp` not | `which`, same |

And it honoured the constraint that made the check meaningful: **the getters read the mounted
`ChangesTab` and `Settings` state rather than recomputing an answer.** A socket that computes its
own result can agree with `git status` while the user's screen shows something else — the
characteristic failure of this codebase, avoided here on purpose.

The consequence is larger than the methods: with no display anywhere, **the critic can now exercise
both surfaces headless**, including the ~20 inventory entries behind Changes.

Also landed, with the same shape of evidence: **P32 (pi)** — `LocalAccountState` so the AI Providers
cards derive their status instead of asserting `"Active"`, a `Radii` token set, the type-scale drift
fixed (12 → 11.5), and a **ledger of every deliberate departure from the frozen waku bar with its
reason**, plus the caveat in the test module's header that conformance to the bar is not proof the
result looks right. Without that ledger the suite would have become an authority over decisions made
for good reasons.

### F-012 — how a real finding hid behind a plausible explanation

The duplicated project rows were **seen and dismissed** hours before the critic explained them. A
capture showed three project rows with identical worktree lists, and the reasoning at the time was
that any checkout of a repo lists all of that repo's worktrees — which is true, and was half of it.

The other half: `restore` re-discovers worktrees under **every** project row, **a linked worktree's
root is itself a repo**, so each discovered worktree becomes a project that discovers the others;
and `write_catalog` never runs on quit while writing two id conventions into one table.

The tell was available all along — **the row count kept growing** — and nobody counted. Which is the
lesson worth keeping: *a plausible explanation is not a verified one*, and the cheapest way to tell
them apart is usually a number measured twice.

Its fix is specified to require **three quit/relaunch cycles with the rows counted after each**,
because one cycle cannot distinguish a fix from a coincidence — the same reasoning that made a
single failed reproduction insufficient to close F-007 at 02:45.

---

## Appended 12:40 — critic pass 4: the pass that overturned a builder

~24 PASSED, and three FAILED verdicts that no one else was positioned to reach.

### The overturned claim — `F-TERM` classification

codex11 reported that of 8 display-blocked `F-TERM` entries, **7 were state-backed and exactly 1
genuinely pixel-only**. The critic exercised them and ruled: **F-TERM-02, 04, 05, 06 and 11 are
rendering entries.** Five, not one. Recorded as *pixel entries counted as state-backed*.

This deserves care, because it is not carelessness. The same builder, in the same report, marked
three entries UNREACHABLE rather than FAILED for absent binaries, and labelled four others
"state-tested but half-proven visually" without being reminded. It held the line repeatedly and
crossed it here. **The boundary between "the program knows this fact" and "the user can see it" is
genuinely hard, and a dead display applies constant pressure in exactly one direction** — toward
counting what you *can* test as what the entry asked for.

Which is the entire argument for having a critic that did not build the piece. Nothing else in this
process would have caught it: the tests pass, the state really is exposed, and the claim is
plausible.

### F-PER-01 — the fifth instance of the characteristic failure

Still FAILED end-to-end. `TerminalStateSnapshot` with capture and replay exists in
`tiller_terminal`; the schema-backed store bounded at 256 KiB exists in the session layer;
**nothing joins them** on save, quit or restore, so a restored terminal still comes up empty.

The critic named the shape itself rather than just the bug: *"the same crate-right, shell-unwired
shape as the Changes surface and badges."* That is now five:

| # | The crate is right | The shell does something else |
|---|---|---|
| F-009 | `select_worktree` maintains control state | the handler only re-highlighted |
| F-010 | `ChangesTab`, 38 tests | mounted by a demo binary alone |
| P16 | a runtime-resolved mono token | five call sites named a macOS font |
| F-008 | `discover_availability()` reads `PATH` | five `"Available"` literals |
| F-PER-01 | capture/replay **and** a bounded store | neither is wired to the other |

### The gate judged, and it held under fire

`ci-linux.sh` was judged honest: its header disclaims visuals, drift is reported separately from
new, the nonce smoke works — and **it correctly refused `CI OK` on a real compile break in the live
tree.** A gate that has never failed is not yet a gate; this one has now failed for the right
reason.

Two sharp edges found in it: the fingerprint hashes `.remember/` runtime files that an ambient hook
rewrites underneath it, producing **two false drift failures**; and the smoke test silently requires
a git checkout at `ROOT`. Both routed. Note the first one's shape — a verification tool made
unreliable by *ambient tooling in the environment*, which is the same class as the four instrument
defects already in this log.

### Settings verified by the cross-check that was designed for it

The socket row shows the live `/tmp/critic4.sock`; Permissions is absent from the four Linux
sections; and **the UI renders the same discovery rows the socket reads** — which is the property
P31 was constrained to have, confirmed by someone who did not build it.

---

## Appended 12:55 — pass 5, and the clearest statement of this codebase's failure mode yet

### The sentence worth keeping

> **The surface lies exactly when git breaks.**

A repository whose `.git` has been removed renders as a clean `0/0/0` with `error=''`, while git
itself says `fatal`. `load_snapshot` swallows every git error, so **F-CHG-09's Retry path is dead
code** — it cannot appear, because nothing ever reports a failure for it to offer to retry.

That is the whole class, stated better than it had been: **an error swallowed and rendered as an
empty state is indistinguishable from success**, and it fails precisely when the user most needs the
truth. The question now in the briefs: *could this exact picture also mean failure?*

Three more from the same pass, all found by trying cases nobody tries:

- No-HEAD staged files show `+0 −0` while their expanded diff shows real lines.
- **`stage` on a conflicted path succeeds and stages the conflict markers.** The Swift original
  refuses. The app helped a user commit a broken file.
- A missing `git` binary reads as "no HEAD".

### It counted, and the brief was wrong

`F-GIT` holds **16 entries, not the 19** my brief claimed — 8 PASSED, 2 PARTIAL, 6 FAILED for
absent behaviour (streaming runner, `branch --list`, clone, remote parsing, directory-status
aggregation, side-by-side). That is the second time an agent has corrected one of my counts, and
both times the correction was right. **A number in a brief is somebody's recollection; the file is
the contract.**

### The predicted hole was there

The critic also noted that `stage`/`unstage`/`discard` are verified inside `tiller_git` and have
**no socket door** — so with no display, the entire `F-CHG` mutation tier is unverifiable by anyone.
It predicted that hole would sit next to the dead error path, and it did. Routed to codex12 (P37)
along with the conflict guard; the two reporting defects routed to pi.

### Closed since pass 5 opened

- **F-PER-01** — bounded scrollback captured on quit, replayed on restore; the nonce survived two
  relaunches. This was the critic's own nomination for the biggest fixable gap.
- **F-012** — project identity canonicalised, linked worktrees deduplicated, legacy rows migrated;
  `projects=1 worktrees=2` **stable across three quit/relaunch cycles**, which is what distinguishes
  a fix from a coincidence.
- **`session.ref`** — now persisted in SQLite rather than living in memory.

---

## Appended 13:00 — two decisions an agent asked for, and why the reasoning matters more than the answer

Twice in an hour a builder stopped and asked instead of guessing. Both questions were good ones, and
both answers are recorded here with their reasoning, because the reasoning is what generalises.

### `stage_all` with a conflicted path: abort, do not skip

**A partial success is the worst of the three options** — worse than refusing and worse than
failing loudly. The user asked to stage everything; staging most of it and skipping some silently is
the same class of defect this project has spent the day removing, and it is the hardest to notice
because the operation appears to have worked.

Aborting is fully recoverable: nothing was mutated, the error names what is wrong, the user resolves
or stages selectively. The cost is an interruption; the cost of the alternative is a wrong index
discovered later. It also follows the rule already set for the single-file case rather than
inventing a second one — **one rule applied everywhere is worth more than two locally optimal
ones** — and the error must name *every* conflicted path, since a user with three conflicts should
not run it three times to find them.

### Crossing into another agent's crate: no, and the reason is not merit

codex12 proposed taking the `changes.rs` error-path work along with its own. Refused — on
boundaries, not competence. pi has been in `tiller_ui` all day and is mid-way through building a
text editor there. **Two agents in one crate is exactly what the ownership rule prevents, and the
failure mode is not a clean git conflict — it is one agent reading a file, thinking for two minutes,
and writing over work that landed in between.**

The split that keeps both moving: codex12 makes the distinction **expressible** — an error type that
can tell a git failure from an empty repository, from a missing binary, from a genuinely clean
tree — and pi makes it **visible**. Same pattern as the `on_action` handoff codex11 wrote rather
than editing `main.rs`, which worked.

### A note on approval gates

Both builders paused for approval before implementing. That is the right instinct on a genuine
design fork and the wrong one on a piece whose acceptance criteria are already written down. The
answer given to both: **the brief's acceptance criteria are the gate; you already have the authority
they grant.** A round trip spent confirming what a brief already authorised is a round trip not
spent on the work — and with four agents and a thirteen-hour session, that adds up.

---

## Appended 13:20 — the capability nobody knew was there

**GPUI ships its own view-test harness, and it works here with no display at all.**
`gpui::TestAppContext` with `VisualTestContext` runs the real element tree, produces a **drawn frame
with a debug-bounds map**, and dispatches real mouse and keyboard events through the real dispatch
path. Elements are found by `.debug_selector(id)` and clicked at their actual laid-out bounds.

pi found it while building the editor, and used it to prove F-CHG-09 the way the entry actually
asks rather than the way that was convenient: a real `.git` removal, the error panel and Retry
asserted **in a drawn frame**, a **real click** on Retry, and recovery after re-init.

### What it changes, stated precisely

| | |
|---|---|
| **Proves** | the element exists in the laid-out frame, at real bounds, and responds to a real click or key **through the real dispatch path** — layout, presence, hit-testing, interaction |
| **Does not prove** | what the pixels look like: colour, font rendering, the comparison against waku |

So a large part of what this project wrote off as `NOT EXERCISED — blocked on display` **was never
about display at all — it was interaction.** "Click the tab and confirm it activates", "press the
chord and confirm the tab changes", "double-click a file and confirm it opens", "open the menu and
confirm the item is listed" — all reachable today.

Appearance stays blocked, stays a debt, and the new capability must not quietly widen into "we can
check the UI now". It cannot check what the UI looks like.

### Why it went unnoticed for thirteen hours

The blockage was diagnosed correctly and thoroughly — the compositor cannot import an AMD tiling
modifier, no display presents, `import` returns one colour — and every response to it was about
**getting a picture out of the running app**: other displays, software Vulkan, native Wayland,
`grim`. The question nobody asked was whether the thing being verified actually required a picture.

**Twelve of the entries needed a click, not a photograph.** The framing "we cannot see the app"
quietly became "we cannot test the UI", and the second does not follow from the first.

Worth setting beside the other lesson of the day: when the evidence you need is unreachable, make
the state observable rather than lowering the standard — and before that, **check whether the
evidence you think you need is the evidence the question actually requires.**

### Two smaller things from the same piece, both worth keeping

- **A named compromise.** The control-socket `ChangesReport` keeps `usize` counts because its
  consumers want numbers, and defaults an uncounted file to zero. That is documented in the module
  header, with the human surface being the honest one. Naming a compromise is what keeps it a
  compromise instead of becoming a bug nobody remembers choosing.
- **This machine's git speaks Italian** (`LC_MESSAGES`). Tests now assert on the locale-independent
  prefix while the user-facing detail shows whatever language git uses. A test asserting on English
  git output would have passed elsewhere and failed here — the worst kind of flake, because it looks
  like a real defect on exactly one machine.

---

## Appended 13:35 — critic pass 6: a PASSED entry that was true and too narrow

### F-013 — the control tier's lifecycle lie

> `workspace.close` leaves control panes' PTYs and **orphaned process groups** running — its own
> `VERIFY` clause demands termination. And `terminate()` kills only the **direct child**, so
> compound-command panes leak on `panel.close` and on app quit too.

**This contradicts a claim already recorded as PASSED.** F-PER-06 was proven with real evidence:
child PID 2708265 alive before the quit, gone after. That measurement was *true*. It watched the
direct child, and a pane running `bash -lc 'foo | bar'` leaves the pipeline behind.

So the failure mode here is not a false claim — it is **exercised narrowly**. The evidence was real,
the method was sound, and the scope was one process wide when the question was a process group wide.
Worth adding to the method rules, because it is subtler than the ones already there:

> **Ask what the smallest true version of your evidence is.** "The child is gone" and "everything
> the pane started is gone" look identical when the pane starts one thing.

It also retroactively explains something everyone misread all day: **this machine has been
accumulating stray app and shell processes for hours**, and that was variously blamed on the
compositor, on agents forgetting to clean up, and on the display wedging. Some of it was Tiller
leaking process groups on every close.

For a program whose entire purpose is hosting other people's processes, leaking them is close to the
centre of what it must not do. Routed as P40, with the evidence required to come from **outside**
the app — pid lists for a whole process group, checked after `panel.close`, after `workspace.close`,
and after a real quit.

### F-014 — a comment that asserts what the code does not do

`worktree.set`'s comments **claim persistence; there is no DB column and it dies on restart.** A
comment asserting a property the code lacks is worse than no comment, because the next reader
believes it and stops checking.

### Confirmed intact

The Codex `-c notify=[...]` override loads in real **codex 0.147.0** — the TOML escaping trap
(`jsonStringLiteral` with `.withoutEscapingSlashes`, because `\/` is not a valid TOML escape) still
holds against a current binary. Worth knowing that a compatibility hazard documented months ago is
still correctly handled and still necessary.

### And the reclamation that now needs auditing

After the `VisualTestContext` discovery, builders went back through everything marked
`NOT EXERCISED — blocked on display` and reclaimed what they judged to be interaction rather than
appearance — across `F-TAB`, `F-EDIT`, `F-TERM`, `F-CHG`, `F-PER`, `F-WIN`, `F-AUTO`.

That is the **highest-risk bookkeeping event of the project**: a large self-assessed
reclassification, made by the people whose work it credits, under pressure from a blockage everyone
wants to be smaller than it is. Pass 7 is auditing it, with the instruction to check both
directions — including whether entries still marked blocked are now reachable, since the builders
had every incentive to reclaim and none to go looking for more work.

---

## Appended 13:55 — an orchestration error of mine: a broadcast that re-tasked two agents

At 13:18 I broadcast the `VisualTestContext` discovery to codex11, codex12 and pireview. The message
ended with an instruction:

> *"Re-examine the entries you wrote off and move the ones that are about behaviour rather than
> pixels."*

That is a **task**, and it went to three agents who were each mid-piece.

codex11 finished its persistence work (`tiller_persistence/{error,lib,db,migrations}.rs` and its
integration tests, 13:04–13:13) and then did exactly what the broadcast said: went into
`tiller_ui/**` and reclaimed F-EDIT-01/09 and F-CHG-03/04/09/10/11/12/14 with drawn-frame evidence,
editing `tab_bar.rs`, `file_view.rs`, `changes.rs` and `right_panel.rs` between 13:41 and 13:50.

**pi was doing that exact piece, in that exact crate, at that exact time** — P39, with its own todo
list showing F-EDIT-01, the Collapse/Expand All tests, the F-TAB chords and the F-EDIT-12 drag
harness.

So for about thirty minutes two agents wrote the same tests into the same files. That is precisely
the failure the file-ownership rule exists to prevent, and no rule failed: **I bypassed it myself**,
by putting an imperative in a channel that does not carry ownership.

### The rule that was missing

> **A broadcast carries facts. Only a brief carries work.**

Briefs are addressed, they state ownership, and they are dispatched after a context reset so the
agent is not mid-anything. A broadcast is none of those things — it arrives sideways, at whoever is
listening, in the middle of something else. Ending one with "now go and do X" quietly re-tasks
everyone who reads it, and none of them can see that the others got it too.

The same message was also sent to codex12 and pireview. codex12 folded it into its own piece
correctly and stayed in its files; pireview was told to audit the reclamation, which is its job.
codex11 was the one whose current piece was nearly finished, so it had room to act — which is
exactly why the damage landed there and not elsewhere.

### Handling

codex11 told to stop, to report its P38 persistence findings (which nobody else has), and to write
the reclamation up as a **handoff** rather than leave it to be discovered as a conflict. pi told to
keep going, to reconcile rather than revert when the handoff arrives, and that **its version wins on
a tie** because it holds the whole design. Neither was asked to throw work away before someone with
the full picture has looked at it.

The cost is one agent-hour of duplicated work and a crate in an uncertain intermediate state. The
cheaper lesson would have been to notice that "re-examine and move" is a verb.

---

## Appended 14:05 — critic pass 7: the audit that justified auditing

The reclamation was audited because it was the highest-risk bookkeeping event of the project. It was
right to audit it.

### F-015 — the `F-TAB` mega-row

> **21 entries under one claim, 13 absent.** The reclassification ran on lists, not per-entry
> checks. **The harness made interaction provable and absence hideable in the same stroke.**

That sentence is the whole finding. A genuine new capability arrived; everyone correctly saw that it
made interaction testable; and in the same motion, thirteen entries for features that **do not
exist** moved out of "blocked on display" into something that reads like progress.

Nobody lied and nobody was careless. The check was performed **per list instead of per entry**, and
that difference is invisible from outside — the summary line looks identical either way. Which is
why the audit had to open each one.

The lesson, now in the briefs as three separate questions asked in order:

1. **Does the feature exist at all?** If not it is `FAILED — absent`, and no harness changes that.
2. If it exists, can it be exercised here?
3. If exercised, does it do what the `VERIFY` clause says — not what it plausibly does?

The reclamation answered question 2 for entries that had never been asked question 1.

### It also looked in the direction nobody was paid to look

Entries still marked blocked that were actually reachable: **`F-SET-03`**'s update click,
**`F-CHG-20`**'s running count, **`F-TAB-15`**'s close control, and **`F-SET-21`**, whose behaviour
half already had a drawn test the audit had missed. The builders had every incentive to reclaim and
none to go hunting for more work; the critic had neither incentive, which is the point of it.

### F-016 — a test-health finding worth more than several entries

> The drawn mutation tests need the full `run_until_parked()` and hardened pump before any entry
> resting on them is ticked.

**A test that passes on timing luck is worse than one that fails**, because it certifies an entry
that may not hold — and it will keep certifying it until someone changes an unrelated timing and the
suite goes red for no visible reason. Routed to pi as blocking: no entry resting on an unhardened
drawn test may be ticked.

### And P38 landed underneath all of it

The persistence layer that had silently carried every persistence claim of the day was finally
tested itself: schema migration, a 64 MiB logical database cap, atomic rollback, session-reference
upsert/load/delete, 22 integration tests.

---

## Appended 14:20 — critic pass 8: nineteen missing features are one missing thing

### F-017 — the shell has no menu bar and no command layer

> That single absence explains **19 of the 29 absent non-browser features** — `F-TAB
> 02/08/09/12/13/14/17/21/26/27`, `F-WIN 02/03/04/05`, `F-SID 07/08/09/12/14`. The only shell
> interaction surface is the tab strip (activate, ✕) plus the `+` menu.

This is the most useful shape a finding can take: not another item on a list, but **the reason the
list is long**. Overflow menu, move tab to pane, Close Others, Close to the Right, rename, resume
chat, the window commands, the sidebar context actions — every one is an operation whose transition
is an afternoon's work and which **no user can reach, because there is nowhere for it to live**.

Nineteen entries had been counted, planned and partly built as nineteen separate gaps. They are one
gap counted nineteen times. Routed immediately, because a builder was in the middle of implementing
those very transitions with no place to hang them: **build the command layer first, then hang the
operations off it.** The existing typed action layer in `panes.rs` and the typed disabled-reason
pattern from `F-TAB-11` are the right foundations — menus need exactly the second one, because a
greyed item with no explanation is indistinguishable from a bug.

### Old verdicts that no longer hold

- **`F-CHAT-24/25`** — the PASSED was for the permission-**card mechanism**; the named Plan and
  Question cards **do not exist**, nor a dozen other chat entries. A verdict that was true of the
  machinery and false of the feature, which is the same shape as the mega-row and the same shape as
  F-PER-06's too-narrow proof. **Three different agents, three different pieces, one recurring
  error: proving the part you can reach and recording it as the whole.**
- **`F-SET-03`** — Check for Updates is a **dead no-op button**, worse than the previous pass had
  it. A control that does nothing is the purest form of this codebase's failure mode: it looks
  exactly like a working one.

### Corrections in both directions

`F-SID` came back 7 PASSED · 2 PARTIAL · 9 FAILED-absent, and **`F-SID-03` and `F-SID-05` were
upgraded from an earlier FAILED** because the surface genuinely changed under them. A report that
only ever moves in one direction is a report worth less; this one moves both ways.

### The test-health hold is cleared

Pass 7's flaky drawn mutation tests (`F-CHG-10/11/14`, `F-EDIT-09`) now pass **deterministically
three times over**, and the chord-dispatch and payload-drag fixtures exist. Entries resting on them
may now be ticked.

### What is still missing after nine passes

**No single artefact says where each of the 388 entries stands.** `INVENTORY-STATUS.md` is a summary
by area, assembled by an orchestrator from reports, and it says outright that its counts are not
evidence. Pass 9 is producing `INVENTORY-LEDGER.md`: one row per entry, with verdict, evidence and
which pass judged it — and `builder-claimed, unverified` kept distinct from `PASSED`, because **an
entry no independent party has touched is not done, however good the builder's evidence was.**

---

## Appended 14:40 — the number, at last, and what it says about the day

`INVENTORY-LEDGER.md` now holds one row per entry for all 388, with verdict, evidence and the pass
that judged it. The totals:

| verdict | count |
|---|---|
| PASSED | **88** |
| half-proven | 46 |
| FAILED — absent | **90** |
| FAILED — defective | 4 |
| UNREACHABLE | 9 |
| N/A — platform | 17 |
| **NOT EXERCISED** | **127** |
| NOT EXERCISED — blocked on display | **7** |

### The display blocks seven entries

Seven. Of 388. **1.8% of the contract.**

That blockage dominated the day: a compositor diagnosis down to the AMD tiling modifier, four
attempted workarounds (other displays, software Vulkan, `MESA_VK_WSI_DEBUG`, native Wayland with
`grim`), three agents reporting it in every summary, an hour of orchestrator time, and a standing
request to the operator to restart their session.

It gates **seven entries**, because the interaction tier was recovered through GPUI's own test
harness once somebody asked whether the evidence actually required a photograph.

Worth being precise about what that does and does not mean. The diagnosis was correct and worth
having; the workarounds were the right things to try; and the seven are real debts. What was wrong
was the *framing* — "we cannot see the app" became "we cannot verify the app", and the second was
never true. **The cost of a wrong frame is not the effort it wastes; it is the effort it never
prompts.** Nobody went looking for a test harness for thirteen hours, because the problem had
already been named as one about pictures.

### What the wall actually is

**127 entries nobody has ever exercised, and 90 features that were never built.** Those two numbers
are the project, and neither had a champion until the ledger existed: the first is invisible because
nothing fails, and the second was partly hidden inside a bookkeeping row.

### Corrections the ledger imposed on the summary

- `F-PER-06` → **FAILED — defective**. Its original proof was real and one process too narrow.
- `F-CHAT-24/25` → **FAILED — absent**. The permission-card *mechanism* passed; the named Plan and
  Question cards do not exist.
- `F-SET-03` → **FAILED — defective**. A dead no-op button.
- `F-AUTO-02..08` → **upgraded** to pass-3-confirmed.
- A vague "≈196 never touched" → a real **127 + 7**.

Three of those five moved *down*, one moved up, and one turned a hand-waved estimate into a number.
A ledger that only ever flattered the project would have been quicker to write and worth nothing.

---

## Orchestrator entry — 2026-08-13 15:2x — two findings worth keeping

### The activity layers are built, tested, and dead (routed as P50)

Critic pass 10 reported that `handle_title_change`, `detect_content_status` and
`refresh_process_signal` have zero callers. Verified independently, and the mechanism is one layer
deeper than the report: **`tiller_terminal` has no title plumbing at all.** `alacritty_terminal`
raises `Event::Title(String)`, nothing forwards it, and `tiller_terminal/src/lib.rs:171` carries a
comment saying title changes would be distinguished *later*. Layer B therefore has no *source*, not
merely no consumer. The app holds one `AgentActivityModel` (`tiller/src/main.rs:1785`) fed only by
spawn events and `tillerctl notify`.

Consequence: **an agent Tiller did not launch, or one whose CLI has no hooks, is invisible** — which
is the entire reason the layered design exists. Three evidence layers activate on one wiring pass.

Sixth instance of the characteristic failure: *a crate does the right thing and the surface does
something simpler and wrong.* All three layers have green unit suites. Unit tests cannot see this
class, and the tell is always the same — **grep for non-test callers of the thing you just proved.**

### A developer's locale silently changed what the program parsed (pi, P44)

`tiller_git` clone-progress parsing returned nothing under an Italian git. `LC_ALL=C` fixed it. The
property that makes this worth recording: the failure was **deterministic on this machine and
invisible on any English one**, so CI elsewhere would have been green forever. Any shell-out that
parses human-readable git output must pin the locale, and the general rule is: *if you parse a
program's output, you own its environment.*

---

## Orchestrator entry — 2026-08-13 15:45 — a false PASSED, and the mechanism that made it

`fable` flagged `F-CHAT-08` (*"See connecting, send, and stop primary action states"*, PASSED at
pass 8) as false. Verified independently: **true, and here is the mechanism.**

`tiller_ui/src/chat.rs:2120-2160` — the composer's primary control is a single `↑` glyph. Its only
variation is `can_send`, which drives a background and text colour. `can_send` is
`!streaming && !connecting && !text.is_empty()`, so **while a turn streams the control is merely
disabled.** It never becomes a stop control. The VERIFY clause asks for three states; two exist and
neither is stop.

**The mechanism is new and worth a name: verdict by adjacency.** Every noun in the VERIFY clause is
present *somewhere* in the file — there is a `connecting` flag and a status pill that renders the
word, there is a `cancel_turn` that really cancels, there is Escape bound to `Cancel`. Three true
facts about three different things were assembled into one verdict about a control that has never
existed. Nobody lied and nothing was careless; the parts were checked and the *conjunction* was
assumed.

This is distinct from the earlier failures. `F-PER-06` was a proof one process too narrow;
`F-CHAT-24/25` was a mechanism that passed while the feature was absent; the mega-row skipped
"does it exist". This one **checked existence of every component and never checked that they were the
same component.**

Consequence, and the reason this is being escalated rather than filed: **146 rows are PASSED, and
every claim this project makes about progress rests on them.** One is now known false, found by an
agent that was not looking for it. Nobody has ever audited that bucket — by construction, since the
critic cannot audit its own verdicts. Routed as FABLE-03.

The cheap defence, adopted into every brief from now on (`fable`'s proposal, FABLE-02): **a claimed
row must name a replayable proof — a test name or a transcript — so the critic replays rather than
re-derives.** A row whose evidence cannot be replayed is the row most likely to be false.

---

## Critic pass 11 — the half-proven bucket is gone (2026-08-13T13:26Z snapshot)

All 64 half-proven rows converted: **19 PASSED, 38 FAILED — absent, 5 FAILED —
defective, 1 NOT EXERCISED — blocked on display**, and `F-CORE-ACT-08` was claimed
by the P50 builder mid-pass (left as builder-claimed, unverified).

The conversions split exactly along the line the brief predicted: **19 rows' missing
halves existed and were exercised** (new drawn tests for the remove-project confirmation
prompt, activity-row click/close, the socket disable effect, usage timeout+PTY-termination;
live exercises for the missing-CLI launch-error surface, close-cancels-registration,
scrollback capture->quit->relaunch->replay, $SHELL preference, and the 1MiB wire cap),
and **38 rows' missing halves are genuinely not built** — most of them one of two
recurring shapes: *a tested model with zero app callers* (planner, partition, eviction,
LayoutCommand, view_state, shell-quote, watcher, default_project_base — seven dead
functions in one pass) and *a control that renders but does nothing* (Copy install
command, Refresh now, provider visibility, six unpersisted settings).

Five rows became **defective**, all in the control tier, all found live:
- `F-CTRL-WIRE-02`: the 1 MiB line cap drains complete lines *before* checking the cap,
  so lines up to 1MiB+64KiB−2 are accepted (pinned live); the Swift server rejects
  anything over 1MiB.
- `F-CTRL-SYS-01`: wire answers `{status:"ok"}` vs documented `{pong:true}`; tillerctl
  prints "pong" unconditionally, so only third-party clients notice.
- `F-CTRL-CLI-01`: the usage line documents `tillerctl [--socket <path>] <command>` but
  the parser errors on that placement (rc=2).
- `F-CTRL-NOTIFY-02`: session+title mode ambiguity resolves to title; the Swift
  NotifyMode errors `ambiguousMode`.
- `F-CTRL-WORK-05`: `workspace.close` leaves the pane PTY running (re-verified: `sleep 60`
  alive after close) — same process-group leak as F-PER-06/F-TERM-08.

**The single biggest gap that is not the display and not the activity-layer wiring:**
the model layer is complete but dead — seven fully-tested models (activity planner,
pane partition, eviction, LayoutCommand, WorkspaceTabViewState, shell-quoted file drops,
the inotify watcher) have **zero non-test callers**, and the settings surface is a
half-wired control panel: Resume sessions, auto-naming, translucency, retention, mount
cap and panel widths are computed, clamped and displayed but **never persisted**
(AppSettings carries 5 of the claimed keys), while Copy install command and Refresh now
are literal no-op buttons. The inventory cannot be finished by tests alone; the next
pass's queue is 38 builder rows.

Snapshot notes: pi's composer files and the control crate were moving during the pass;
rows touching them carry the 13:26Z snapshot time. A machine-wide disk outage invalidated
some transient build/test results mid-pass; every affected result was re-run green after
the cleanup (279G reclaimed), and one remaining artifact (the p18-auto worktree the
scrollback replay used) was deleted by that cleanup after the evidence was captured.

---

## Critic pass 12 — the audit applied, the ledger's arithmetic fixed (2026-08-13, late afternoon)

Applied `PASSED-AUDIT.md` (fable). Snapshot `/tmp/critic-pass12`, shared
`CARGO_TARGET_DIR=/tmp/critic-target`, `TILLER_SOCKET=/tmp/critic12.sock`; all suites
run from the snapshot (tiller_ui 140/140 green incl. 3 new critic tests; tiller_git
57/57 green; tiller_terminal context-menu test green).

**The ten false PASSEDs: 10/10 confirmed, 0 overturned.** Each disproof re-checked
against the live tree (line numbers moved since the audit; all still valid). The
mechanism is as fable named it — verdict by adjacency: true facts about several
different things assembled into a verdict about a thing that does not exist.
F-CHAT-02 (no auth state anywhere; `auth` appears once, in a test's fake-agent
script), F-CHAT-08 (one ↑ gated only by `can_send` — never stop; P53's socket-door
stop transcript preserved in the row, the UI clause re-marked FAILED — absent),
F-CHAT-16 (no search/no-match/Recommended), F-CHAT-18 (percent+tokens+Cost only,
no input/output/cache rows), F-CHAT-23 (ToolCall{id,title,status}, static row,
no expand/links/Dismiss), F-SID-15 (menu ends at New Chat; hover-× removal has
no confirmation), F-SET-16 (static "Search agents" text; Refresh handler is the
literal no-op `|_, _, _| {}`), F-SET-21 (exactly one Files icon choice on Linux),
F-USE-02 (`tooltip` has zero matches in status_bar.rs), F-USE-03 (every
Unavailable reason renders the same "—"). All ten → FAILED — absent, pass 12.

**The four unreplayable PASSEDs.** F-SID-01/02/04: new named drawn critic tests
added in the snapshot and green — `projects_header_add_control_and_project_rows_render`,
`filter_narrows_rows_and_clearing_restores_them`,
`project_chevron_hides_and_restores_children` (sidebar.rs; a `filter-field` debug
selector was added to make the filter queryable). Kept PASSED on the new evidence.
F-USE-01 downgraded FAILED — absent: the refresh control never renders —
`on_refresh` is a dead builder API with zero call sites; render is gear + three
hardcoded segments + branch·path; zero tests in status_bar.rs.

**The four doubts.** F-TAB-15 → FAILED — absent (no tab context menu exists; the
palette "Close Tab" is drawn-tested at main.rs:6313 but is not the clause's
surface). F-AUTO-06 → FAILED — absent (create/list/clear live; the delivery
conjunct has no path — F-USE-06's zero callers). F-WIN-01 → FAILED — absent
(gear/socket settings real; no `ctrl-,` chord in any crate). F-WIN-07 →
FAILED — absent (restore real; the History-menu route is absent and the row
disclosed it — a disclosed-absent clause cannot stand as PASSED). F-TAB-10 kept
PASSED with upgraded evidence: the clause's pane-menu route now exists — drawn
`right_click_resolves_this_terminal_and_draws_all_context_actions` green
(tiller_terminal lib.rs:1751) + shell split wiring (main.rs:2325-2341).

**Totals recomputed from the body (all eight buckets):** PASSED 152 · half-proven 0 ·
FAILED — absent 130 · FAILED — defective 12 · UNREACHABLE 7 · N/A — platform 23 ·
NOT EXERCISED 38 · NOT EXERCISED — blocked on display 8 · builder-claimed 18 ·
total 388. Never-judged: 54 (22 builder-claimed + 7 P50 + 25 never claimed).

**F-SID-14 flipped FAILED → PASSED** (drawn context-menu dispatch test green +
shell/palette wiring code-verified) — the mirror defect of a false PASSED, found
one of each today. Four more stale FAILEDs in the same class re-marked: the
terminal context menu now exists (context_menu.rs, 10 items, drawn test green),
so F-CORE-TERM-02, F-TERM-04, F-TERM-06, F-TERM-UI-01 → NOT EXERCISED (menu
drawn-proven; per-action clipboard/clear effects not individually exercised),
and F-TAB-26 + F-TERM-05 → PASSED (drawn menu test + shell routing +
`terminal_context_app_actions_have_workspace_routes`).

**Six stale F-GIT FAILEDs (routed by fable's FACT) → PASSED.** All six modules
exist and have named tests, all green in the fresh snapshot:
streaming runner (`streaming_runner_delivers_stderr_before_the_child_exits`),
branches, clone (progress + invalid-source failure), remote parsing,
directory-status precedence, side-by-side pairing (2 tests) — all in
tests/p41_git_behaviors.rs. Zero app callers for all six — package capability
proven, wiring owed; same dead-code class as the seven pass-11 models. Negative
greps were positive-controlled per the FACT's caution.

**Pass-11 findings: already ledger rows.** All seven dead models
(F-CORE-ACT-24/25/26, F-CORE-DOM-03, F-CORE-WSP-04, F-CORE-WSP-08,
F-CORE-FILE-03, F-CORE-FILE-06), the six unpersisted settings (F-CORE-SET-01 +
F-SET-04/05/06/07), the no-op buttons (F-SET-08 Copy install command, F-SET-10
Refresh now, F-SET-16 agents Refresh — re-marked this pass), and all five
control-tier defects (F-CTRL-WIRE-02/CLI-01/NOTIFY-02/SYS-01/WORK-05) were made
FAILED rows in pass 11. Zero-caller claims re-verified in the moved tree with
positive controls. Convergence noted: fable found the Refresh no-op by reading
settings.rs:1187; pass 11 found it by exercising the surface — one defect, two
methods, no coordination.

**Two deterministic P50 test failures** (re-run in isolation, still red; not
disk-related): `process_refresh_preserves_process_ownership_until_process_gone`
(panes.rs:636 — passes the test process's own pid as the shell pid; the /proc
walk finds no agent child and clears the Running state the test expects
preserved) and `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second`
(panes.rs:962 — first OSC title not suppressed inside the 1500ms Layer-A window),
plus a suite-teardown SIGABRT from a PTY reader thread. These sit under
F-CORE-ACT-07/09/10/11, which stay builder-claimed. Also observed:
`a_permission_prompt_answers_both_ways` flaked once under parallel load (green in
isolation and in the full re-run) — not in FLAKY_TEST_FINDINGS.md.

**Concurrency:** the live tree moved during the pass (P52/P53 moved F-CHAT-01/07/08
and F-PER-01 to builder-claimed). A green drawn Escape test
(`escape_cancels_the_stream_and_the_transcript_states_it`, chat.rs:3670)
contradicts part of P53's "drawn Escape path remains unwired" claim — flagged
here; the row stays builder-claimed, not PASSED, so no re-mark.

**Backlog:** the 64-row half-proven bucket was fully converted in pass 11 (ENOSPC
interrupted only the tail of that pass; the conversion landed — 0 half-proven
rows remain). What remains for future passes: 38 NOT EXERCISED + 18
builder-claimed rows. EVIDENCE-STANDARD.md did not exist when this pass started;
the rule from the brief was applied directly: a verdict made by reading code is
not a verdict — UI rows need a named drawn test, machine rows a named test or
executed transcript, appearance rows a shot.

**Disk:** `/tmp/critic-pass11` (58G) deleted at end of pass;
`/tmp/critic-target` (4.1G) shared and reused; `/tmp/critic-pass12` kept for the
next pass to replay the three new critic sidebar tests.

## PASS 13 — the display is back; the /tmp-proof rule is now law

**Structural rule applied: critic tests land in the live repository.** The three
pass-12 sidebar tests died with the reboot (`/tmp/critic-pass12`), which made
F-SID-01/02/04 look proven while being exactly as unreplayable as before. They are
re-established as live-repo tests and the verdicts cite the new names:
`projects_header_add_control_and_project_rows_render`,
`filter_narrows_rows_and_clearing_restores_them`,
`project_chevron_hides_and_restores_children` (all sidebar.rs, green 15/15).
The pass-11 remove-project confirmation test had the same /tmp disease (its name
was cited by F-SID-10 while the test itself was gone); replaced by
`remove_project_context_item_confirms_before_emitting` (sidebar.rs, green).
One production line added for testability: the filter field now carries a
`debug_selector("filter-field")`, matching every other control.

**F-PRJ — the 18-row family, finally judged.** Exercised on the live display with a
real catalog (socket transcripts + frames in `reference/linux-progress/2026-08-13/`):
the Add flow is a directory picker and nothing else. F-PRJ-02 PASSED (socket add →
catalog row → sidebar rows; picker→event drawn-test green). F-PRJ-01/03..18
FAILED — absent, each with its own reason: no clone form, no create form, no
non-git prompt, no insertion-error surface (eprintln only), settings sheet is
read-only (no trash, no name edit, no icon UI, no base/location controls). The
whole family moved out of "never claimed".

**The 8 display-blocked rows lost their excuse; 0 remain.** F-CHG-02 FAILED — absent
(exercised: no current workspace + changes.open still serves the last worktree — no
no-worktree state exists). F-CHG-20 FAILED — absent (builder claim confirmed by
reading: no running-count/activity element in changes.rs). F-CHG-06 half-proven
(transcript data half + status dots photographed; no named drawn test for the tree
decorations). F-SET-19/20 half-proven via the new live-repo drawn test
`appearance_controls_drive_theme_translucency_and_font_size` (settings.rs, green);
pixel halves owe a light-theme frame — the XTEST click would not land and the socket
has no settings write door. F-SET-22 FAILED — absent (agent colour swatches are
display-only; no choice exists). F-TERM-09 half-proven: `panel state` reports
running (transcript), but the running→finished transition is pixel-identical
(AE=0, frames TERM-11/12) — no visible indicator change. F-PER-07 FAILED — absent
(icon/name editing doesn't exist; no persistence door to exercise).

**F-SID-12 (primary transition) — half-proven.** Catalog half green:
`set_primary_flips_the_application_level_marker` (session.rs) — siblings cleared
per project, unknown path errors, other projects untouched. The full-route drawn
test `worktree_primary_context_transition_reaches_the_catalog` is written (main.rs)
but the whole tiller-bin gpui suite is currently blocked by a **zbus
non-determinism flake**: gpui_linux's `listen_for_system_wake` executor wakes during
any bin gpui test and trips the scheduler's determinism guard — pre-existing
`probe_escape_dispatch` fails identically when run alone. Needs an owner: the wake
listener should not run under the test scheduler (or a builder should gate it).

**Harness findings (both fixed in-tree this pass):**
- `visual-sweep.sh` gap list is stale — it claims no Changes/Settings socket methods
  while `surface.changes.open/read/...` and `surface.settings.open/select/read` exist
  (all exercised this pass). The script also predates the catalog-gated
  `workspace.select` (added `project.add` first) and had a strict-PID-only window
  finder while the compositor reparents the app under a frame without `_NET_WM_PID`
  (added the linux-shot warned-geometry fallback). Also: a relative `--out-dir`
  broke the app-log redirect after the `cd "$FIXTURE"`; absolute paths work.
- **sccache serves stale rmeta**: a cached pre-cosmic `tiller_theme` rmeta made
  `tiller_ui` report `unresolved import tiller_theme::cosmic` although the module
  exists. Deleting the stale artifact (or `RUSTC_WRAPPER=`) fixes it. Worth knowing
  for every cold-build-on-shared-target session.
- **XTEST click delivery**: `xdotool click 1` batches press+release so gpui's
  `on_click` never fires; split `mousedown`/`mouseup` with a delay works. Right-clicks
  (mouse-down handlers) always worked. The + click opens a real portal
  (ashpd FileChooser request appears on the D-Bus; the dialog is Wayland-side and
  invisible to X captures).
- **Display hygiene**: two stacked instances (a killed-but-alive earlier launch at
  +2905+235 and a new one at +2905+379) made every click land on the wrong window for
  a long stretch; geometry-only window discovery is not safe on this shared display,
  and zombie frames outlive their processes. All frames in the manifest were
  re-verified single-instance.

**Appearance tier (quantitative, MEASURED.md tokens):** column dividers pure black
1pt at x325/x1064 — MATCH. Sidebar bg #181818 vs reference canvas #131417; title bar
#272727 vs #141416 (noticeably lighter); status bar #1A1A1A vs #141416. The
side-by-side waku comparisons for all five sweep frames are in
`reference/linux-progress/2026-08-13/visual-sweep/comparisons/`. Typography-level
debt still needs human eyes; the colour deltas above are measurable now.

**In-flight pieces left untouched per the brief:** chat.rs (pi D1 — its
`stopping_via_click_with_a_queued_item_still_sends_it` is red mid-TDD), persistence
(codex11), main.rs chat wiring (codex12). The tiller-ui lib failed to compile at
several points this pass purely from in-flight edits; my tests were verified in
green windows between them.

## PASS 14 (pireview, ~20:30–23:00)

- **The goal's acceptance test is exercised and PASSED, for both ACP agents.**
  Driven through the real UI on :1 (picker → chat tab → composer → Enter): Codex —
  session JSONL `~/.codex/sessions/2026/08/13/rollout-…22-13-58….jsonl` carries my
  UI-sent prompt and the reply `CRIT14-CODEX-DONE-3317`; the spawned stack was
  `codex-acp → codex.js app-server → codex app-server` (q1-ps.txt) — the per-adapter
  `acp_program` fix is live on the New Chat path. Claude — `~/.claude/projects/
  -tmp-critic14-cwd/514189c4….jsonl` carries prompt + reply `CRIT14-DONE-8219`, with
  `claude-agent-acp` on ps. Frames: reference/linux-progress/2026-08-13/pass14/.
- **The picker gates correctly, live**: `m2-01-picker-open.png` shows exactly two
  chat rows (Claude Code, Codex) — `is_available() && acp_program().is_some()`
  (tab_bar.rs:467). Environment had claude/codex/pi on PATH; pi is correctly
  excluded for ACP.
- **NEW FINDING (live) — no composer auto-focus, structural**: a freshly opened
  chat tab does not put the cursor in its own field; keystrokes before the first
  click are silently swallowed (`s1-02-typed-noclick.png`: typed `NOFOCUS-PROBE-7777`,
  transcript stayed empty and the placeholder stayed; one click in the field, then
  the identical send worked and the agent replied). Root shape: `Chat` IS focusable
  (Focusable → composer_focus, chat.rs:3040) and `select_pane` (main.rs:4339) does
  resolve+focus it — but `add_chat_tab` (main.rs:3934) takes no `&mut Window`, so
  creation cannot focus structurally; all four `select_pane` callers are click/key
  paths, none creation. Owner: pi (chat.rs / add_chat_tab).
- **GAP 1 confirmed live (restore/resume = Claude default)**: at one app launch,
  4× `claude-agent-acp` spawned — one per restored chat tab — while the DB held a
  Codex tab (`n2-ps.txt`; 5 tabs in critic14.sqlite). `TabRecord.agent_id` (P70)
  exists but every row is NULL — the writer half in main.rs is still open.
  `resume_chat`/restore call `Chat::launch`, and set `agent_icon/agent_id: None`,
  so a label check cannot catch it — only process identity can.
- **Orphaned adapters**: killing the app leaves `npm exec …-acp` node processes
  alive (observed 4+); no cleanup path.
- **tiller bin red, owner-named**: 8 failing tests, growing while the author
  edits: drawn_palette ×2, drawn_disabled_save_row, ctrl_k ×2,
  escape_closes_the_settings_surface (main.rs — codex12 live), plus
  process_owned_status_survives…, real_pty_layer_a_debounce… (panes.rs — P50 author).
  Also the clippy gate is red at cosmic/live.rs:111 and cosmic/theme.rs:88
  (sonnet fixing) — the orchestrator corrected an earlier all-clear; the gate's
  own invocation is the only probe that counts. Not reported as build failures.
- **Display harness lessons (cost ~1h)**: the XTEST pointer on :1 is shared by all
  agents — clicks race; the import pipeline serves slow/stale captures; and my
  stage-checker shipped a wrong-path bug (`/tmp/crit14` vs `/tmp/critic14`) that
  looked like an app defect. The self-correcting click→shot→verify driver
  (per-stage retries + pointer-position verify) is the pattern that survived.
- **F-SET-09 re-marked defective**: settings Install Skill button renders with an
  empty handler `|_, _, _| {}`; the tested provisioner (`tiller_project/skill.rs`)
  has zero consumers.
- **F-WIN-06 flipped per the user's scope ruling**: browser is in scope;
  `NewTabAction::NewBrowser` is an empty match arm yet the menu still offers
  "New Browser" (silent dead entry) while the socket answers browser.* honestly.

## PASS 15 (pireview, ~22:30–23:55) — FABLE-09 recipes

- **39 recipes → 9 rows converted or refreshed live, 4 verdicts changed**
  (F-TAB-07, F-CHG-05, F-SET-02, F-CHAT-08 to PASSED; F-TAB-08, F-CHG-03 to
  half-proven; F-CHG-11, F-TERM-PTY-05 re-evidenced FAILED). Ledger
  188 → 192 PASSED, 131 → 125 FAILED-absent.
- **Recipe-vs-screen mismatches (information about the census):** (1) the
  "status-bar gear" door does not exist — `StatusBar.on_settings` is never
  wired to a rendered element (status_bar.rs render has no settings button);
  settings opened via the control socket instead. (2) The tab-strip chip
  right-click and all right-click recipes are unreachable on this display
  session (XTEST button 3 produces no app state change; pass 13's session had
  it working). (3) ctrl-o/Open File goes through the Wayland-side portal
  picker — invisible to X captures; editor rows rest on drawn tests.
  (4) F-TERM-PTY-05's recipe drives `sleep 8` — even with input delivered via
  `tillerctl panel write`, running-vs-done frames are byte-identical: the
  activity wiring the row says is absent, stays absent.
- **F-SET-02 split recorded:** Escape closes settings LIVE (634k px) while
  the workspace drawn test still fails — the palette-test harness cluster in
  the tiller bin (owners named pass 14) is broken, not the feature.
- **F-CHAT-08:** live streaming captured (20 bands → 19 after stop click);
  the agent completed too fast to observe an active-stream cancel — the
  drawn + socket-door stop tests are the cancel transcript.
- **Keyboard delivery unlocked:** XTEST key events land only after a real
  click gives the app X input focus (windowactivate alone does not work on
  this compositor); inside a focused PTY, chords are swallowed by the shell
  (mac-only chord candidate, 88193ce).
- Build note: workspace was red at pass start (codex11's uncommitted P72
  gtk/wry deps); orchestrator installed the system libs; binary rebuilt at
  22:41 including P73. All drives after that used the current tree.

## PASS 16 (pireview, ~23:45–00:30)

- **P73 LIVE-PROVEN — the pass-14 "largest remaining gap" is closed.** Codex chat
  opened via the picker → DB row `agent_id='codex'` (pass 14: all NULL) → quit +
  relaunch → ps shows exactly ONE codex-acp (the Codex tab) and ONE
  claude-agent-acp (the generic NULL-agent Chat tab — correct default). No
  wrong-agent processes; no orphan accumulation at launch. Frames/ps:
  reference/linux-progress/2026-08-13/pass16/.
- **Group-0 test replays: 7 of 8 converted.** ACT-05/08/09 PASSED (their cited
  panes:: tests ok); ACT-27 PASSED (0 ignores, panes:: executes 16/2, terminal
  suite green); **ACT-06/07/11 → FAILED — defective**: their own cited tests
  (`process_owned_status_survives_title_and_child_exit_events`,
  `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second`) fail
  reproducibly (pass 14 and 16; panes.rs untouched since 19:39 — stable, not
  transient). codex12 is verifying whether the tests or the code are wrong —
  those three verdicts may come back. ACT-10 (no test) rides the kill-9 live
  half, not reached.
- **omp re-marked**: the real package ships `oh-my-pi` with no `omp` alias
  (package.json bin verified); omp.rs:35/65/77 hardcode "omp" →
  F-AGENT-OMP-01/02/03 = defective-by-name, not environment. OPENCODE-03 →
  FAILED — absent (no summarizer-command generator exists anywhere in the port;
  opencode 1.18.18 is installed and runs).
- **Right-click cap reached** (2 attempts, both with focus-first per the
  orchestrator's hypothesis): tab-chip and pane context menus do not open on
  this display session. Chronicle only — ENVIRONMENT.md already states XTEST
  button 3 does not reach the app under this Wayland session; no rows were
  marked FAILED on it.
- **Display degradation**: load ~30 during the late drives; settings-surface
  clicks produced 0-px deltas (theme segments unresponsive or mis-hit) and the
  chat-stage clicks missed repeatedly — Group 2 (F-CHAT-03/05/15/20/29/30/33)
  and F-SET-19/20 remain for the next pass. F-CHG-06 partial: status symbols
  measured per row (staged green #B3D9B7 vs amber variants); the post-`git add`
  Refresh click was inconclusive under load.

## PASS 17 (fable, ~00:19–02:30)

Critic role held by fable this pass — both pi panes died on 429 GoUsageLimitError at
00:19 and the orchestrator will not spend money while the user sleeps; fable was the
only panel that never wrote a line of Rust here. Deviation from "the critic runs on
pireview" recorded openly in `tasks/CRITIC-pass17-handover.md`. All drives on the
`TILLER_DB` fixture (`p17-small-session.sqlite`), real state DB untouched; all frames
`reference/linux-progress/p17-*`; decisive process/kill transcripts copied into
`reference/linux-progress/p17-artifacts/`.

- **Group 2 closed — all eight rows exercised live with a real claude over ACP.**
  F-CHAT-03 PASSED (offline banner + Retry respawns the bridge), F-CHAT-33 half-proven
  (turn-error half incl. machine-reason JSON), F-CHAT-20 half-proven (tail-follow),
  F-CHAT-05 half-proven (offline-inert without placeholder), F-CHAT-15 FAILED — defective
  (pill displays, chooser never opens), F-CHAT-29/30 FAILED — absent (no copy controls),
  F-CHAT-13 NOT EXERCISED (instrument wall: xdotool cannot synthesize XDND; the "+"
  route opens the portal picker, invisible to X). SET-19 Light proven end-to-end
  (click → repaint → DB → relaunch first-frame), SET-20 FAILED — defective.
- **False PASSED overturned: F-PER-01.** Two completed UI chat exchanges → `chat_turn`
  0 rows, `session_ref` 0 rows (WAL-aware read), empty transcript after relaunch
  (p17-ai0). Settings/tab_state survived the same SIGTERM — not debounce loss. The
  pass-14 tests prove the store and the socket door; the UI chat path never calls
  either. The DB read caught what the frame alone would have passed.
- **Translucency is a dead control, three independent anchors** (SET-20):
  `set_translucency` never calls `changed()` (settings.rs:871-874, unlike every
  sibling); `SettingsSnapshot` has no translucency field (:338); zero consumers of the
  flag anywhere (no WindowBackgroundAppearance/Blurred). Live: only the knob repaints
  (p17-ae3); no DB key ever written. Would have been marked PASSED from the frame alone.
- **F-TERM-09 → FAILED — defective, full state catalog measured** across four live
  agent launches: `?` tab badge at boot, amber ● on the worktree row at first launch —
  then NO working indication during a real turn (`✳ Orchestrating…` live, badge simply
  gone, p17-as2), no idle/done return after it, and the ● NEVER clears: it survived
  the agent's death and an app relaunch that restored the tab as plain bash
  (p17-ap0..3). No Linux analogue of the Swift processGone clearing is wired.
- **Layer-A can never work in-product as shipped**: every hook of a Tiller-launched
  Claude Code fails `/bin/sh: 1: tillerctl: not found` — SessionStart,
  UserPromptSubmit, and all 3 Stop hooks, reproduced across 3 independent launches
  (p17-aq1/aq2/as2/as4). The adapter's worktree-local hook config IS written and the
  hooks DO fire; the binary is not on the spawned shell's PATH (F-CTRL-CLI-02's live
  consequence; feeds TERM-09's dead indicators).
- **Rowless defect — the worktree context menu's agent items ignore their anchor row.**
  Prospective proof: menu opened on the NON-primary linux/gpui-waku row (Set Primary
  present in the menu), its "Claude Code" item clicked → the tab and TUI spawned under
  the PRIMARY rust/gpui-rewrite worktree, cwd `~/Scrivania/Progetti/tiller`
  (p17-aq0/aq1). Same misdirection had produced the AO launch. Primary-vs-selected
  disambiguation is one grep in the dispatch code away — behavior proven, wiring not
  chased. The menu itself renders at a FIXED top-left position (22,110)-(300,385)
  regardless of which row was right-clicked (aq0 vs an5 identical geometry).
- **Rowless finding — persisted terminal-agent tabs restore as plain bash.** A "Claude
  Code" tab that had a live TUI restores after relaunch as a bare shell in the correct
  worktree cwd (powerline `tiller → rust/gpui-rewrite`, p17-ap0) — no resume attempt.
  Coherent with `session_ref` never being written (nothing to resume from). Chat tabs
  DO respawn their adapter (F-TAB-27, pass 16) — the gap is terminal agent tabs only.
- **Three stale "zero callers" corrected on tonight's builder work**: main.rs now
  routes activity transitions through `should_notify` (:3892) → `build_payload`
  (:3917) → `notify-send` (:1732). ACT-19/20 moved FAILED-defective → NOT EXERCISED;
  ACT-02/NOTIFY-03 blocked-half wording updated. Wired ≠ delivered: no notification
  observed live (visible-window suppression during all drives) — correction of fact,
  not promotion. The QUEUE.md staleness method applied the hour it was written.
- **Copy chords are defective, not absent — the grep flipped my own verdict in-flight.**
  chat.rs binds `ctrl-a`→SelectAll and `ctrl-c`→CopyTranscript with the correct
  modifier and the comment "cmd- forms would bind Super and be unreachable" (:659-661),
  `copy_transcript` is `.on_action`-wired (:3722/:3738) — yet live: no visible
  selection, paste-check empty twice with the composer focused (which binds the chord).
  The transcript is entirely uncopyable on Linux: chords dead in practice + no
  per-message control + no block control (grep over chat.rs and tiller_markdown).
- **Retry drops the turn** (ACP-08): the error entry's Retry respawns the bridge and
  resets the pill but never re-sends the failed prompt — idle at +20s, the user's
  message silently vanishes. Distinct from the restart-works half (CHAT-03 PASSED).
- **Instrument facts measured this pass** (added to ENVIRONMENT.md): input events
  QUEUE during post-restore main-loop congestion and deliver late IN ORDER (ar1≈ar2
  4s apart, menu still open ≥24s after rclick, then ar3 shows the whole queued
  sequence executed — allow 15-20s settle after a many-pane relaunch); first-frame
  paint lag reproduced twice more (ap2/ah1 — a no-change frame is never evidence by
  itself); `xdotool type` renders an em-dash as the literal text "nosymbol" — ASCII
  only in typed prompts; error-banner Y position depends on how much streamed before
  death, so Retry coordinates must come from the same-run frame, never a
  differently-populated layout (the AK misclick).
- **Fixture-DB methodology validated**: `TILLER_DB` pointed every drive at a scratch
  SQLite; the persisted state accumulated across drives (each run's tabs visible to
  the next) which twice shifted sidebar row geometry under reused coordinates — read
  the current frame before anchoring clicks. Real DB untouched all night.
- **Money/scope note**: all agent turns were trivial-deterministic (count sequences,
  "ok"); the wrong-worktree defect landed three claude TUIs in the Swift reference
  repo — zero writes there (prompts were no-tools; transcript saving off via inherited
  CLAUDE_CODE_CHILD_SESSION marker), and the repo's git state was never touched.
- **ACP appendix added to the ledger**: 14 behavior rows found by exercising the chat
  surface, kept OUT of the pinned 389 denominator per the user's ruling. Report format:
  "N/389, plus 14 newly-found ACP rows not yet in the denominator".
- **Transplant gate run at ledger time**: `Scripts/transplant-check.py` exits 1 with 46
  candidate runs. Character on read: 35+ are GPUI `Element`-trait boilerplate
  (icons.rs/chat.rs/lib.rs implementing trait-dictated signatures — `request_layout`/
  `source_location`/style defaults) plus standard idioms (percent-decode loop, hex-digit
  match, HSL formula, `.file_name().map().filter()` chains). No product-logic block among
  them, but per the script's own rule each still owes a human read — none has been
  cleared here, only characterized.
- **Ledger totals regenerated** (`ledger-totals.py --write`): 389 rows exactly — PASSED
  204 · half-proven 18 · FAILED-absent 110 · FAILED-defective 18 · UNREACHABLE 3 ·
  N/A-platform 22 · NOT EXERCISED 13 · builder-claimed 1. Never-critic-judged: 24.

## PASS 18 (fable, ~02:35–03:25) — FABLE-09: the editor keyboard tier

- **The builder's claim holds.** Printable characters land at the caret, reach disk on
  `ctrl-s`, and every gesture in the brief behaves: Backspace, Delete (caret and across a
  selection), Enter splits without submitting, typing replaces a selection. The proof is
  per-gesture disk read-backs (`reference/linux-progress/f09-*-*.txt`), not screenshots of
  text; no verdict in this pass rests on a frame alone.
- **Two stale FAILED — absent flipped by drive**: `F-EDIT-03` (the `Large file — manual
  preview` bar exists on a 396021-byte file and `Render preview` actually renders,
  g1→g3) and `F-EDIT-05` (the conflict banner is mounted on focus-regain; Keep keeps the
  buffer and lets `ctrl-s` overwrite, Reload adopts disk and discards the local edit —
  f6/f7/f9 plus `f09-f-keep.txt`/`f09-f-reload.txt`).
- **`F-EDIT-06` re-anchored**: its PASSED evidence had proved plain save — `F-EDIT-04`'s
  clause. The actual clause (delete externally, edit the buffer, `ctrl-s` recreates) is
  now driven: deleted-file banner (`This file was deleted. Saving will recreate it.`, no
  Reload/Keep), and the save recreated the file — 109 bytes back on disk
  (`f09-h1/h2.png`, `f09-h-recreated.txt`).
- **Editor semantics worth knowing (by design, not defects)**: clicking a source line
  selects the whole line (`select_source_line`) and the next keystroke REPLACES it —
  insertion needs a caret first (`Home`, arrows). A CLEAN buffer adopts an external
  change silently on refocus, no banner (`check_external` reloads clean docs) — the
  banner is the dirty path only.
- **Interference event, mechanism verified**: at 03:02:56 the orchestrator's
  control-socket probe hit MY app instance, and a `browser.*` request against an
  instance with no browser surface auto-creates a Browser tab at example.com
  (main.rs:4614-4619, read and confirmed) whose native webview then paints over the GL
  surface (P72-class). The contaminated g-frames were discarded and re-driven on a fresh
  fixture DB. Standing hazard: the socket is per-instance — a probe against a machine
  running someone else's fixture app contaminates their captures.
- **File tabs do not survive relaunch**: every fixture-DB restart logs
  `[session] tab "aaa-f09-scratch.md" (file) is not restorable in this build; skipped`
  (in the committed `f09-*.log` files) — F-PER territory, flagged for the owner, not
  judged in this pass.
- **Replay kit**: fixtures live at the worktree root, uncommitted scratch —
  `aaa-f09-scratch.md` (every content state is recorded in the committed read-back
  `.txt` files) and `aaa-f09-large.md` (any Markdown > 256 KiB reproduces it). Drives
  used `TILLER_DB` fixture SQLites; coordinates in the ledger rows belong to a 1715x972
  window with the Files panel open.

## PASS 19 — P92: the twenty-eight rows a drawn test cannot see (fable, 2026-08-14 ~04:20)

Held session on the fixture DB (`TILLER_DB=/tmp/p92.sqlite`, app pid 1101962, lock
`fable-p92-held` with holder pid = app pid). Scratch repo `/tmp/p92src` cloned via the
UI to `/home/enzopalmisano/p92src`, worktree `p92wt`. All frames `reference/linux-progress/p92-*`.

**Coverage against the brief**: Tier 1 — 3/3 driven live (F-CHG-19/21, F-EDIT-09).
Tier 2 — 10/10 driven against real ACP agents (claude-agent-acp AND codex-acp, both
spawned as app children; per-conjunct records + agent-side JSONL payload checks per the
Amendment). Tier 3 — 2/13 driven live (F-SID-03/13 before the reprioritization order);
**11 deferred with honest count** (F-CHG-04/09/10/12/14, F-TAB-02/03/04/09, F-WIN-04,
F-EDIT-13) per the orchestrator's mid-session directive to protect Tier 2's budget.
Tier 4 — 2/2 re-checked by grep (F-SET-03/08, both hold; one stale sub-claim corrected).

**Verdict deltas (mine)**: `F-CHAT-12` PASSED → **FAILED — defective**. Everything else
re-confirmed or recharacterized in place. (The Totals shift UNREACHABLE 21→13 /
NOT EXERCISED 21→29 is the stale block absorbing the orchestrator's earlier body edits,
not this pass.)

**The new defect (F-CHAT-12): chip removal leaves the composer keyboard-dead.**
Removal itself is correct at both levels (chip gone from view; next payload carries no
file part). But after clicking the ×, five recovery gestures a real user would try
(click field, type, Return, Escape, click, type) all delivered nothing — four identical
frames — while `echo hi` typed into the Terminal tab of the SAME window executed
seconds later, proving the app's keyboard pipeline alive. Only leaving and re-entering
the chat tab restores input. chat.rs:3786's mouse-down `composer_focus.focus()` does
not win it back. Drawn tests are blind here by construction: `remove_chip` mutates the
model correctly.

**Recharacterizations worth knowing**:
- `F-CHAT-36`: no agent in the roster exposes an empty model catalog — after one turn
  BOTH real agents upgrade badge→picker. The no-models trigger is roster-unreachable;
  badge verified live only as the pre-turn state.
- `F-CHAT-17`: effort levels are real and host-supplied (six choices), chip label
  tracked XHIGH→HIGH→MAX→XHIGH live; picker gate (`has_completed_turn`) seen from both
  sides. "Max" sits at the panel's clipped right edge — clicking it also fires
  mouse-down-out and closes the picker (cosmetic-adjacent, not filed as a row).
- `F-CHAT-11`: attach dispatch proven at the D-Bus level (OpenFile, parent
  `x11:400001`, accept_label "Attach image", **multiple:false** — multi-rejection is
  delegated to the portal dialog). Portal `Request.Close` from a third party: Access
  denied. The dangling dialog dies with the app (Request lifetime = sender lifetime).
- `F-SET-03`: "zero updater code" now stale — `tiller_project/src/ui.rs` carries a
  transport-less UpdateState machine, zero consumers; surface absence still asserted.

**Instrument facts new this pass** (ENVIRONMENT.md candidates):
- The @-mention popup's filter repaints one frame late; the pre-filter list is a stale
  paint (cost me one misplaced click at a moved row).
- The composer text field does NOT regain focus from chip-control clicks; always click
  the field before typing after any chip interaction (and after the ×-trap, a tab
  roundtrip is the only recovery).
- An open modal portal dialog does NOT block XWayland app input under this compositor;
  a "blocked" frame after a portal open is ordinary paint lag (second-capture rule).
- Trivial streams are fast: a 200-line count completes in <5 s; use ≥1000 lines to
  reliably hit a mid-stream window.

**Anomalies recorded for other rows' owners** (not mine, not verdicts): waku's Chat
tab and the alpha.txt file tab drop on restore; worktree terminal spawns in app cwd on
creation but correct cwd on restore-respawn; status bar gained a "Fable" usage segment.

**Residue**: fixture repo+worktree left at `/home/enzopalmisano/p92src{,-p92wt}` and
`/tmp/p92src`. The volatile evidence is copied into the repo so verdicts stay replayable:
D-Bus capture at `reference/linux-progress/p92-dbus-filechooser.log`, and the cited
agent-side payload records (queue drain, both interrupts, `compact_boundary
trigger:'manual'`, the alpha.txt resource link, the no-file-part send, the
post-New-Conversation send) extracted to `reference/linux-progress/p92-payload-evidence.jsonl.txt`
— full session JSONLs remain at `~/.claude/projects/-home-enzopalmisano-p92src-p92wt/`.
App, lock and portal dialog released at session end.

## PASS — F-CHAT-05 critic, fresh session (chat05, 2026-08-18 ~10:45–11:05)

- Judged F-CHAT-05's unproven half: the permission-wait clause (the offline half was
  already live-proven in the prior pass). Live-drove it end to end on
  `Scripts/wayland-drive.sh` (`TILLER_WL_LABEL=chat05`, nested Wayland lane on
  `wayland-1`) against the real installed `claude` CLI over ACP — no fixture stand-in
  needed for this row.
- **Unblocked the prior pass's "environment-blocked" note.** A worktree-local
  `.claude/settings.local.json` with `{"permissions":{"defaultMode":"default"}}` in a
  disposable scratch repo (`/tmp/chat05-repo`, outside the project tree; `.claude/` is
  gitignored) overrides the user's global `defaultMode: auto` for that one worktree —
  `ClaudeCodeAdapter::prepare` (`rust/crates/tiller_agents/src/claude.rs`) only merges
  the five hook arrays and leaves any existing `permissions` key untouched; read back
  unchanged after boot to confirm. Prompting the agent to write a file then produced a
  genuine ACP `permission_request` for the Write tool — not a bypass auto-approval, and
  not the Plan-mode ExitPlanMode path either (the composer showed "Opus Plan Mode"
  throughout, but Claude still asked for the actual Write, Deny/Allow Once/Always
  Allow card and all).
- **Proof, not a screenshot that merely contains the control.** At the pending moment
  (`reference/linux-progress/f-chat-05-permission-wait-pending.png` — composer's own
  "Waiting for permission response…" placeholder, "Question waiting · Write …" bar,
  Deny/Allow Once/Always Allow card in the transcript) I clicked the composer, typed a
  marker (`ZQX-CRITIC-MARKER-PERM-9932-should-not-appear`) via real wtype keystrokes,
  and pressed Return. The next frame
  (`reference/linux-progress/f-chat-05-permission-wait-blocked-type.png`) is
  pixel-identical to the pending one — no marker text landed in the composer, no
  "Queued: …" chip appeared, transcript entry count unchanged: the whole editor really
  is out of service, matching `ChatComposerView.canInteract`'s exclusion and the
  `permission_wait_disables_the_composer_and_shows_its_own_placeholder` unit test, now
  confirmed live. Clicking Deny then resolved it correctly: the tool card flips to
  "Failed", the permission card reads "Answered: Deny", Claude's own follow-up
  acknowledges the refusal ("Got it — I won't create that file…"), and the target file
  never touched disk — confirmed after teardown with `ls`/`cat` (absent) and
  `git status --short` in the scratch repo (clean) —
  `reference/linux-progress/f-chat-05-permission-wait-denied-settled.png`.
- **Timing trap worth recording** (WAYLAND-LANE.md candidate): the gap between "user
  turn sent" and "the actual `Entry::Permission` lands" is not fixed. One run had the
  Deny/Allow card up by t=6s; another was still `Preparing file… / Pending` (tool
  announced, no Deny/Allow buttons yet) at t=6s and only resolved into the real
  permission ask a few seconds later. A marker typed during that gap lands in the
  ordinary D-CHAT-03 mid-turn "queue for next turn" slot instead — visibly, as a
  removable "Queued: …" chip. That is correct F-CHAT-06 behavior for that earlier
  moment, not a violation of F-CHAT-05, but it is pixel-similar enough to the real
  target state to misjudge from a fixed sleep. Always wait for the visible Deny/Allow
  card before running the refusal gesture, not a fixed timer.
- **A second, unrelated, higher-severity trap this pass tripped over — flagged for
  whoever owns worktree/tab bootstrap, not scored against any row here.** On a truly
  fresh `TILLER_DB` (the first-ever `project.add` for a path, `"added":"true"`), the
  new worktree's auto-created default Chat tab launched its ACP agent bound to the
  *app's own initial cwd* (the real project checkout this instance was started from),
  not the new worktree's path — even though the sidebar, Files panel and the
  composer's own cwd label all correctly showed the new worktree as selected. A prompt
  sent through that tab reached the **real project repo**, and because that checkout's
  effective settings are the user's global `defaultMode: auto`, Claude wrote a file
  there with no permission gate at all before I caught it (deleted immediately;
  `git status --short` confirmed nothing else was touched — the tree's pre-existing
  unrelated changes from other concurrent agents were left alone). Killing and
  rebooting the app once against the same, now non-empty (`"added":"false"`), DB
  reliably fixed the cwd on every later boot this pass. This matches the already-
  recorded PASS 19 anomaly ("worktree terminal spawns in app cwd on creation but
  correct cwd on restore-respawn") but extends it: it also hits the **Chat** tab, and
  Chat's version has a live-agent side effect, not just a cosmetically wrong prompt.
  Anyone driving `surface.chat`/agent panes against a fresh DB should reboot once and
  verify the cwd label before typing anything a live agent could act on.
- Host/lane: x86 desktop, nested Wayland (`Scripts/wayland-drive.sh`,
  `TILLER_WL_LABEL=chat05`); no `chat_fixture.py` stand-in used for this row — the real
  `claude` CLI produced the permission request throughout. Scratch repo
  `/tmp/chat05-repo` (untracked, outside the project tree) and its
  `.claude/settings.local.json` override are disposable and were not committed.

**Verdict (mine): `F-CHAT-05` half-proven → PASSED.** Both halves are now live-proven
against the real reference-literal behavior: offline (prior pass) and permission-wait
(this pass) each disable the composer with their own distinct placeholder and refuse
typed input outright, and the permission-wait half also resolves correctly through a
live Deny with no side effect on disk.

## PASS — F-CORE-FILE-03A critic, fresh session (cfile, 2026-08-18 ~11:25-11:35)

- Judged against the row's own VERIFY clause ("Drop several files in a known order,
  including a provider that resolves slowly, and confirm classification and insertion
  preserve that order"), not the builder's narrative. Re-drove live, own filenames, on
  a workspace build I built myself (`cargo build --manifest-path rust/Cargo.toml
  --workspace`, exit 0) — did not trust the binary from the ledger entry.
- **Own-name discriminator, ordinary speed**: `xdnd` from (600,250) to (600,505) on the
  Terminal pane's prompt with `/tmp/cfile-fast-BRAVO.txt` then `/tmp/cfile-fast-ALPHA.txt`
  (deliberately non-alphabetical drop order). Prompt read
  `'/tmp/cfile-fast-BRAVO.txt' '/tmp/cfile-fast-ALPHA.txt'` — correct order, both paths,
  frame `02-20-fast-order.png`.
- **Slow-provider discriminator, `--delay-ms 500`** (above the ~150ms race window the
  root-cause doc names): same gesture with `/tmp/cfile-slow-ZULU.txt` then
  `/tmp/cfile-slow-YANKEE.txt`. Prompt read
  `'/tmp/cfile-slow-ZULU.txt' '/tmp/cfile-slow-YANKEE.txt'` — not lost, correct order,
  frame `04-22-slow-order.png`. Confirms the row's own reproduction (prompt previously
  stayed empty at this delay, per `P133-gpui-xdnd-slow-provider-race.md`) is fixed, and
  that fixing the slow case did not cost the fast case — both ran in the same session
  back to back with a `chord ctrl u` clear between them.
- **Position half of my brief's item 3** — a slow-provider (`--delay-ms 500`) drop whose
  drag started at (350,80) (up in the pane's neofetch banner, nowhere near an input) and
  walked with real intermediate motion to (600,505) (the prompt) landed the text exactly
  at (600,505), not at the stale entry point — `'/tmp/cfile-move-XRAY.txt'
  '/tmp/cfile-move-WHISKEY.txt'` sitting cleanly at the prompt cursor, frame
  `06-14-moved-drop.png`. Backed by reading (not just running) the unit test the fix
  added for exactly this: `a_pending_drop_submits_at_the_latest_tracked_position_not_the_
  entry_point` constructs a `DragState`, advances `drag.position` past the entry point,
  marks a `PendingDrop`, and asserts `pending_drop_submit_position` returns the *latest*
  position, not the first — a real assertion against the production function, not a
  tautology.
- **Trap I hit and want on record for `WAYLAND-LANE.md`'s `xdnd` section**: `xdnd <x1>
  <y1> <x2> <y2> …` with `x1==x2 && y1==y2` (anchor and drop at the identical point)
  reliably prints `CANCELLED` with no `TARGET`/`DROP_PERFORMED` at all — the pointer
  never leaves the `xdnd-source` overlay surface mapped at that point, so Tiller's
  window underneath never gets `wl_data_device` focus. Reads exactly like an app-side
  drop failure and is not one; use a real offset between anchor and target, always.
  Cost me two throwaway runs before I caught it from the source's own log echoed by the
  action.
- **Independent build/test verification, not carried from the ledger's numbers**:
  `cargo test --manifest-path rust/vendor/gpui_linux/Cargo.toml --lib` → `34 passed; 0
  failed`, output read directly, both new regression tests present and green. `cargo
  test --manifest-path rust/Cargo.toml -p tiller_terminal -p tiller_ui -p tiller --lib`
  → `45 passed; 0 failed` / `344 passed; 0 failed`, no panics, no `FAILED`. The vendoring
  does not appear to have broken anything else it touches.
- Host/lane: x86 desktop, nested Wayland (`Scripts/wayland-drive.sh`,
  `TILLER_WL_LABEL=cfile`), binary pinned at `/tmp/cfile-tiller` before driving, built
  fresh from `15cebcbf` at the top of this pass. Frames under `/tmp/cfile-shots/` are
  local, not committed (not asked to be; this file is the durable record). Test files
  (`/tmp/cfile-{fast,slow,move}-*.txt`) are disposable scratch, outside the repo.

**Verdict (mine): `F-CORE-FILE-03A` half-proven → PASSED.** Both VERIFY-clause halves
(order preservation, ordinary and slow-provider) are live-proven with a discriminating
drop order and my own filenames; the position-tracking claim in the builder's story is
independently confirmed both live and by the production-function unit test; the fast
case is not regressed; and the vendored crate's own test suite plus the three
consuming packages closest to the change are all green under a build I ran myself.

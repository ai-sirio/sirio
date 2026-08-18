# Critic pass on wave K — eight builder claims, judged live (`wf-judge`)

Fresh critic, did not build any of the work under judgment. Host: the x86 desktop described in
`ENVIRONMENT.md`'s 2026-08-18 section. Lane: the nested Wayland lane (`Scripts/wayland-drive.sh`)
plus a hand-rolled dbus/notification-daemon rig for the F-CORE-ACT-20 gap. Binary pinned once from
current HEAD and reused for every drive in this pass:

```
git rev-parse HEAD          # 4c9f552164f34a555e4a98ba05408890bb9ef737
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm, ~34s
cp rust/target/debug/tiller /tmp/wf-judge-tiller
export TILLER_WL_BIN=/tmp/wf-judge-tiller
```

Every row's evidence below was personally driven by this pass unless explicitly marked otherwise.
Screenshots referenced below are committed under `reference/linux-progress/waveK-critic/`.

## Environment note: the box hit literal 0 bytes free, repeatedly, mid-pass

`ENVIRONMENT.md`'s "keep concurrent lane-driving agents at about 5" was well exceeded during this
pass — `ps` showed at least 8 concurrently-running `tiller` instances under distinct labels
(`wf-act`, `wf-chg`, `wf-tab`, `wf-rest`, `wf-rest2`, `wfj-notifhost`, `wf-sweep`, plus this pass's
own) at the point this pass started. The root filesystem (`/`, which is also `/tmp`) went from
`df`-reported 2.2G free at pass start to **repeated, sustained stretches at literally 0 bytes
free** — not momentary, but for minutes at a time, several separate times. At 0 free, every tool
in this harness that needs to write output (Bash, Write) fails outright, including a bare `true`;
only `Read` kept working. Concrete effect: this pass lost a meaningful fraction of its wall-clock
budget waiting out these stretches, and had to delete ~2GB of clearly-orphaned debris (dead
`wf-fix-tiller-*` binaries from the already-committed, already-merged wave-I builder pass, plus
this pass's own prior interrupted attempt's leftover `/tmp/wf-judge-{PREFIX,DEBUG}-tiller`
binaries — confirmed via `fuser`/`ps` that nothing live held them) twice, by hand, just to get
enough headroom to run `git commit`. **This same scratchpad session directory
(`.../scratchpad/wfjudge/`) already contained a prior, interrupted attempt at this exact task**
(same lane label, same eight rows, screenshots timestamped up to 21:07 today) — it died mid-drive
on `F-CORE-ACT-20`'s real-daemon gap without ever writing a report. Its leftover screenshots were
reviewed as a sanity check where legible, but every row below was independently re-driven by this
pass on this pass's own pinned binary; the predecessor's `F-SID-19` screenshots in particular were
internally inconsistent (a `Chat`+`Terminal` tab pair visible in what should have been an empty
zero-tab state) and are **not** relied on for any verdict below.

---

## 1. CENTER-01 — the safety gate: a needs-input pane must survive a worktree switch (mandatory gap)

Commits `790096b0`/`1abc7b2c`, doc `CENTER-PANE-DESYNC.md`. Both confirmed ancestors of HEAD
(`git merge-base --is-ancestor`). The builder's own doc names this exact gap unproven: "exercise
the safety-gate path live (put a pane in `NeedsInput` via `tillerctl notify`, switch away, confirm
the pane survives with its live PTY rather than a fresh empty one) — the unit test proves this in
isolation but was not independently redriven live in this pass." This is the brief's mandatory gap;
driven here over the real control socket with a **hard discriminator: a host PID that must not
change**, not a screenshot alone.

**Setup**: two fresh git repos, `/tmp/wfj-c01-repoA` and `/tmp/wfj-c01-repoB` (`git init` +
one commit each), added as two Tiller projects via `ctl project.add`. `TILLER_WL_LABEL=wfjc01`,
`TILLER_WL_BIN=/tmp/wf-judge-tiller` (this pass's own fresh build of current HEAD,
`4c9f552164f34a555e4a98ba05408890bb9ef737`).

**Drive**:
1. Selected repoA (`ctl workspace.select workspace=/tmp/wfj-c01-repoA`), opened a new terminal tab
   (`chord ctrl t`), clicked into it and typed `sleep 600` + Return — a bare running command, no
   agent, exactly F-TERM-10/CENTER-01's shared scenario.
2. Confirmed on the **host** the child process is real and running:
   `ps -p 173355 -o pid,etimes,args` → `173355 6 sleep 600` (screenshot
   `center01-02-sleep-running.png`).
3. `ctl panel.list worktree=/tmp/wfj-c01-repoA` → the active pane's id is `pane-2`.
4. `tillerctl`-equivalent socket call: `ctl notify session=pane-2 status=needs-input` → `{"queued":
   "true"}`.
5. `ctl workspace.select workspace=/tmp/wfj-c01-repoB` → `ok`, path/branch confirm the switch.
6. Immediately re-checked the **same host PID**: `ps -p 173355 -o pid,etimes,args` →
   `173355 25 sleep 600` — **same PID, `etimes` advanced continuously from 6 to 25 with no gap**,
   i.e. the process was never killed and respawned. Forced-repaint screenshot
   `center01-03-repoB-selected-gate.png` shows the sidebar highlight, the Files panel path
   (`/tmp/wfj-c01-repoB`) and the status bar (`master · /tmp/wfj-c01-repoB`) all agreeing the app
   really switched to repoB, while the **centre pane still shows repoA's `sleep 600` terminal** —
   the documented "stale-but-safe" behaviour, not a bug: the reload that would tear down and
   replace the centre pane's tabs is the exact thing the gate skips.
7. Three more switches (`repoA → repoB → repoA`, i.e. 5 total switches from the first), then a
   final host check: `ps -p 173355` → `173355 53 sleep 600` — still the same PID, `etimes` still
   advancing continuously. Screenshot `center01-04-after-more-switches.png` shows repoA reselected
   with the identical terminal content still mounted.

**Verdict: PASSED.** The hard discriminator (a real host PID that must not change) held across five
worktree switches while the pane was marked `needs-input`: the live PTY was never torn down and
replaced by a fresh one. This is my own fresh drive against this pass's own pinned current-HEAD
binary, not a reused screenshot. I did not separately re-drive the plain forward-case reload
(content resets on a clean switch) since that half is not in dispute and the builder's doc already
demonstrates it live; the safety gate itself — the half nobody had driven — is what this pass
closes.

---

## 2. F-CORE-ACT-20 — notification suppression must follow REAL window focus (mandatory gap)

Commit `1abc7b2c` (ancestor of HEAD), doc `CENTER-PANE-DESYNC.md`. The builder's own gap: its live
proof used "a private, ad hoc D-Bus session bus **because the box's real user session bus was
down**" with "a minimal Python `org.freedesktop.Notifications` stub" — and it explicitly asked for
this to be "repeat[ed]... independently" with "a real notification daemon (not a stub) in the loop
... to rule out any difference between a stub answering `Notify()` and a real listener." Driven here
with the **real, installed `notification-daemon` (GNOME) 3.20.0 package** — not a stub — on a
private bus, per the brief's own recipe.

**Rig** (all on one throwaway, isolated bus — the operator's real `/run/user/1000/bus` was never
touched):
1. `dbus-daemon --session --fork` → private `DBUS_SESSION_BUS_ADDRESS`.
2. A dedicated tiny nested sway (`WLR_BACKENDS=headless`, `xwayland enable`) just to give the real
   GTK3 daemon somewhere to run — `GDK_BACKEND=wayland` alone segfaulted this build
   (`Gtk-WARNING **: cannot open display:` then SIGSEGV under the headless pixman Wayland path);
   `DISPLAY=:N` (Xwayland, lazily started by that same private sway) + `GDK_BACKEND=x11` runs
   cleanly.
3. Launched the real binary: `/usr/lib/notification-daemon/notification-daemon` with the private
   `DBUS_SESSION_BUS_ADDRESS` and that `DISPLAY`. Positive control before touching Tiller:
   `dbus-send ... GetServerInformation` → `"Notification Daemon" "GNOME" "3.20.0" "1.2"` (confirms
   the real daemon, not a stub, owns the name); `notify-send "sanity" "..."` → exit 0.
4. `dbus-monitor --session` attached to the same private bus, logging every frame verbatim.
5. Tiller (`TILLER_WL_BIN=/tmp/wf-judge-tiller`, this pass's own current-HEAD build) launched via
   `Scripts/wayland-drive.sh` with `DBUS_SESSION_BUS_ADDRESS` exported into my shell first (the
   script's `env` invocation for the app process does not clear it, so Tiller inherits it —
   verified: `post_desktop_notification` in `main.rs` just `Command::new("notify-send").spawn()`s,
   so it uses whatever `DBUS_SESSION_BUS_ADDRESS` its own process environment carries).

**Drive**, one worktree (`/tmp/wfj-c01-repoA`), one terminal tab (the only/active tab throughout,
so `pane_visible` is true the whole time):
1. `title ✳ notif-test` on the pane (OSC 0) — confirmed via a fresh forced-repaint screenshot the
   tab picked up the Claude-style `✳` icon and the sidebar worktree row lit up with the matching
   glyph (Layer B title identification, title-owned).
2. **Baseline, window genuinely focused** (Tiller is the only mapped window; confirmed later by
   contrast, see step 3): `ctl notify session=pane-0 status=needs-input` → queued. No new bus
   traffic (ambiguous case — `should_notify(None, NeedsInput, app_active=true, pane_visible=true)`
   is `false` either way; not yet a discriminating step, matches the builder's own baseline).
3. **Window genuinely defocused**: launched a second real Wayland client, `foot`, into the *same*
   nested compositor Tiller runs in. `swaymsg get_tree` on that compositor's own socket confirms
   `foot` `focused: true` and Tiller's own window node `focused: false` at this instant — a real
   compositor-level focus change, not a flag. Pane still the visible/active tab (untouched).
   `ctl notify session=pane-0 status=done` (genuine transition, `needs-input` → `done`) →
   **`dbus-monitor` captured a real `Notify` method call**, verbatim (full transcript committed at
   `reference/linux-progress/waveK-critic/act20-real-daemon-notify-transcript.txt`):
   ```
   method call ... sender=:1.20 -> destination=:1.6 ...; interface=org.freedesktop.Notifications; member=Notify
      string "Tiller"
      uint32 0
      string ""
      string "Claude Code — wfj-c01-repoA/master"
      string "master · wfj-c01-repoA"
      ...
   method return ... sender=:1.6 -> destination=:1.20 ... reply_serial=9
      uint32 2
   ```
   `:1.6` is the real notification-daemon (same connection that answered `GetServerInformation`
   with `"Notification Daemon"/"GNOME"/"3.20.0"` earlier on this same bus); the `uint32 2` reply is
   a genuine notification id allocated by the real daemon's own internal counter, not an echoed
   stub value.
4. **Window refocused**: `foot` killed; `swaymsg get_tree` confirms Tiller's window node
   `focused: true` again (sway auto-refocused it). Another genuine status change,
   `done` → `error`: `ctl notify session=pane-0 status=error` → queued, then a forced repaint. The
   monitor log's line count is **byte-identical before and after** (784 lines both times) — zero
   new bus traffic of any kind, let alone a second `Notify` — confirming suppression is restored by
   real focus alone, with pane visibility held constant across all three steps exactly as the
   builder's methodology intended.

**Verdict: PASSED.** This closes the named gap precisely: the same fire/suppress contrast the
builder demonstrated with a stub now holds against the real, installed GNOME notification daemon —
a real `GetServerInformation` handshake, a real `Notify` call with a real allocated id, and true
silence (not just "no visible popup") when the window is genuinely focused. Rig detail worth
recording for the next critic: this daemon's GTK3 build segfaults under `GDK_BACKEND=wayland` on a
headless/pixman compositor — use `DISPLAY=:N` + `GDK_BACKEND=x11` against a private sway with
`xwayland enable` instead.

---

## 3. F-SID-19 — ctrl-t from the empty state; does the fix generalize?

Ledger line 88, commit `407f7c9b` (ancestor of HEAD). The wave-I builder's own re-drive confirmed
Ctrl+T from the zero-tab "No Terminals" state; its named gap: "the fix's own rationale generalizes
to 'every global keybinding' reachable from root — a fresh critic should drive at least one other
root-level keybinding... from the same zero-tab empty state to confirm the generalization holds."

Read `handle_root_key_down` (`main.rs`) first to pick a second real root-dispatched binding rather
than guessing: it also gates `Ctrl+Shift+P` (always) and `Ctrl+K` (unless a terminal is focused) to
`open_command_palette` — the same capture-phase root handler family Ctrl+T's fix targets, so it is
a genuine second instance of "a root keybinding with nothing focused," not a coincidence.

**Drive**: fresh scratch git repo (`/tmp/wfj-sid19-repo`), added as a project, selected — zero tabs,
landing on "No Terminals" (screenshot `sid19-01-empty-state.png`). **Without ever pressing Ctrl+T**,
sent `chord ctrl+shift p` directly from that empty state. Result
(`sid19-02-palette-from-empty-state.png`): the full Command Palette opened — a real, populated
command list (New Terminal Tab, Open File, Save File, Toggle Sidebar, Focus Pane Left/Right/…,
Jump to Tab 1–9, …), not a blank or inert overlay.

**Verdict: PASSED.** The fix generalizes: a second root-level keybinding, reachable only through
the same root-focus path Ctrl+T's fix repairs, fires correctly from the identical zero-tab empty
state with nothing else focused. This is my own fresh drive on this pass's own pinned binary,
against a brand-new scratch project (not reusing any prior state).

---

## 4. F-TERM-10 — the negative case: a genuinely idle shell must still reload

Ledger line 328, commit `984defa7` (ancestor of HEAD). Wave-I builder re-confirmed the positive
case (a live `sleep 300` survives) live; its named gap: confirm the fix "didn't overshoot into
pinning every terminal forever regardless of activity" — a genuinely idle bare shell (no foreground
child) should still get reloaded/recycled on switch-away, same as before this fix. This is the exact
contrapositive of CENTER-01's safety-gate row above (§1), driven back to back on the same
mechanism for a clean before/after pair.

**Drive**: fresh repos `/tmp/wfj-term10n-repoA`/`repoB`. In repoA's terminal, typed
`echo IDLE_MARKER_A` + Return and let it **complete** — the shell returns to a bare prompt with no
foreground child (screenshot `term10-negative-01-idle-marker.png`). Identified the pane's real host
shell PID by walking Tiller's own process children (`ps --ppid <tiller-pid>`): `285565`, confirmed
alive (`etimes 10`). `ctl workspace.select` to repoB, then back to repoA. Read the pane back over
the socket (`panel.read id=pane-0`, raw PTY bytes, not a screenshot) — result: a **fresh neofetch
banner with no trace of `IDLE_MARKER_A`**, i.e. the tab was genuinely reloaded with new content, not
left stale.

**Verdict: PASSED**, on the row's own clause (an idle pane's *visible content* reloads rather than
being pinned). One side observation, noted but not counted against this verdict since it is outside
what the clause asks: the original host PID (`285565`) was still alive and still a direct child of
`tiller` (`ps --ppid`) a minute after the round trip, even though it was no longer the content
behind `pane-0`. Whether that is an intentional grace period or a minor PTY-reap lag is a separate,
narrower question from "does the idle pane visibly reload" — it does — so I record it here as a
lead for a future pass rather than reclassifying this row.

---

## 5. F-PRJ-13 — Reset button click leaks through to the row underneath

Ledger line 106, commit `043b1e71` (ancestor of HEAD). Wave-I builder's own gap: its live re-drive
proved general click-occlusion (any in-sheet click), "which is a slightly broader claim than the
original bug report (specifically about Reset)... A fresh critic could tighten this by reproducing
the exact original repro shape — a decoy worktree row rendered directly under the Reset button's own
pixel position in a live three-project sidebar — matching the regression test's fixture geometry
exactly."

**Reproduced the exact fixture geometry live**: two projects, `prj13-decoy` (12 real git worktrees,
`git worktree add`, alphabetically sorted so `decoy-3` is the 7th sidebar row) and `prj13-target` (a
bare git repo, no worktrees). Right-clicked `prj13-target`'s header, clicked "Project Settings",
which opened the sheet for `prj13-target` while `prj13-decoy`'s own rows kept rendering underneath —
`decoy-3`'s row lands directly under the icon-picker's Reset button (`project_identity.rs:720`,
`id("project-icon-reset")`) at pixel `(161, 508)`, confirmed both by the sheet screenshot and by a
**positive control**: a plain click at `decoy-3`'s own row position (`113, 479`, sheet closed)
flipped `ctl workspace.list`'s `decoy-3` entry from `selected:"false"` to `selected:"true"` — proving
this specific machine-readable field is a live, sensitive discriminator, not an inert one.

**Post-fix, current HEAD** (`/tmp/wf-judge-tiller`, this pass's own binary): clicked Reset at exactly
`(161, 508)` — the pixel position `decoy-3`'s row occupies underneath the sheet. `ctl workspace.list`
immediately after: **every worktree in both projects, `decoy-3` included, still reads
`selected:"false"`** — no leak, matching the fix's intent exactly at the fixture's own geometry.

**Pre-fix binary, built and reproduced myself**: `git worktree add --detach /tmp/wfj-prefix-prj13
984defa7` (the commit immediately before `043b1e71`, confirmed via `git log`), built with its own
`--target-dir` (cold build, 3m50s, kept isolated from the shared `rust/target` four siblings were
using). Drove the **identical** live gesture (same coordinates, same fixture) against this pre-fix
binary: surprisingly, `ctl workspace.list` again showed no selection change, even after retrying at
five different y-offsets (485–520) spanning the whole row — a live discrepancy against the fix
commit's own claimed live pre-fix reproduction, worth recording rather than silently smoothing over.
Rather than leave that live ambiguity as the only evidence, I went to the **code-level** reproduction
the task brief explicitly allows: spliced the *exact* regression test
(`reset_button_click_does_not_leak_through_to_the_row_underneath`, copied verbatim from current HEAD)
onto the pre-fix source tree (`984defa7`'s `sidebar.rs`, which has neither the test nor the
`.occlude()` fix) and ran it there:

```
cargo test --manifest-path rust/Cargo.toml -p tiller_ui --lib --target-dir /tmp/wfj-prefix-prj13-target \
  -- reset_button_click_does_not_leak_through_to_the_row_underneath
```
```
thread '...' panicked at crates/tiller_ui/src/sidebar.rs:5583:9:
clicking Reset must not also select whichever decoy worktree row is rendered underneath it -- got
[ProjectSettingsChanged(...), SelectWorktree("/tmp/prj13-decoy/wt-4")]
test result: FAILED. 0 passed; 1 failed
```
Genuinely RED — a real leaked `SelectWorktree` event, exactly the defect the commit describes. Same
test against current HEAD (`rust/crates/tiller_ui/src/sidebar.rs` as committed):
```
test sidebar::tests::reset_button_click_does_not_leak_through_to_the_row_underneath ... ok
```
Genuinely GREEN. This is a real, executed red→green pair at the exact commit boundary — a valid
reproduction under this task's own rules ("build from an older commit into a scratch location") —
even though my *live-UI* attempt at the same pre-fix binary did not itself surface the symptom.

**Verdict: PASSED**, with the live/code discrepancy above stated plainly rather than hidden: the
fix is proven both by an executed red→green test targeting the exact original defect, and by my own
live re-drive at current HEAD with the exact original fixture geometry and a hard discriminator
(`workspace.list`'s per-worktree `selected` field, positive-controlled). What I could **not**
reproduce was the *live, pre-fix* UI symptom itself (my own drive against the pre-fix binary showed
no leak at any of six tried y-offsets) — this doesn't weaken the fix's proof (the code-level red run
is unambiguous), but a future pass attempting a live pre-fix repro should know this and not assume
the live symptom reproduces trivially; it may depend on details (project count, click sequencing)
this pass's fixture didn't match.

Screenshots: `reference/linux-progress/waveK-critic/prj13-01-settings-open-on-target-decoy-underneath.png`,
`prj13-02-after-reset-click-sheet-unchanged.png`.

---

## 6/7. F-PRJ-17 / F-PRJ-18 — default worktree base and worktree location controls

Ledger lines 110–111, commit `2d66b3ce` (ancestor of HEAD, built by the wave-I builder pass itself,
not a pre-existing fix). Named gaps: F-PRJ-17 — "the 'Use Primary' shortcut button itself was
exercised only by the drawn unit test... a fresh critic should click it live and confirm the field
snaps to the primary branch name and that this also persists across reopen." F-PRJ-18 — "the
folder-picker 'Choose...' flow... was exercised only by the drawn unit test's mocked path-prompt...
a fresh critic should click Choose..., select a folder in the real picker, and confirm the location
field updates and persists."

**Setup**: fresh scratch git repo (`prj1718`, primary branch `main`), one project, right-click →
Project Settings.

**F-PRJ-17, "Use Primary" — driven live, both halves.** Typed `featurebranch` into the branch-search
field: the field updated to show `featurebranch` / `Pinned` (screenshot `prj1718-02-typed-pin.png`).
Clicked the real "Use Primary" button (`(277, 606)`, via the persistent virtual-pointer FIFO directly
against the still-running instance — confirmed with an ack, not assumed): the field snapped back to
`main` / `Following primary branch (main)`, and the branch-search field cleared
(`prj1718-03-after-use-primary.png`). Closed the sheet and reopened it (fresh right-click → Project
Settings): **still `main` / `Following primary branch (main)`** — persisted, not just an in-memory
redraw (`prj1718-04-reopened.png`). Independently confirmed via a direct read-only query against the
live on-disk DB: `default_worktree_base` reads `None` for this project's row (`NULL` = "follow
primary" — exactly what "Use Primary" is supposed to write), not the leftover `featurebranch` string.

**Verdict for F-PRJ-17: PASSED.** Both the click and its persistence are independently confirmed live,
closing the named gap exactly.

**F-PRJ-18, typed location — persistence independently reconfirmed** (I made a mid-drive editing
mistake here worth recording rather than hiding: after typing a first path, a `ctrl+a`+backspace
"clear" attempt didn't visibly take in the screenshot I captured immediately after — I initially
misread this as a possible bug, but the close/reopen capture and a direct DB read both show the
*final* typed value, `abcxyztestpath`, cleanly and consistently in both places, meaning my
intermediate screenshot simply raced the repaint (the documented "the first capture after an action
often shows the frame from before it" trap) — not an app defect. `worktree_location_override` in the
live DB reads `abcxyztestpath`, matching the field shown on reopen exactly.)

**F-PRJ-18, "Choose..." folder picker — attempted, NOT EXERCISED.** Clicked the real "Choose..."
button (`(275, 757)`) against the live running instance. No visible change in the captured frame, no
error in the app log, `worktree_location_override` unchanged in the DB — consistent with
`WAYLAND-LANE.md`'s documented limitation that the portal file picker is Wayland-side and invisible
to this lane (no `xdg-desktop-portal` runs inside the nested headless compositor, so
`cx.prompt_for_paths` has nothing to talk to). I could not drive this leg, and say so rather than
inferring a verdict for it from the surrounding evidence.

**Verdict for F-PRJ-18: half-proven.** The typed-path leg of the conjunction (type → persists across
reopen) is independently confirmed, live, with a DB cross-check. The `Choose...` native-picker leg is
`NOT EXERCISED` — this lane has no portal to answer it, and (per `ENVIRONMENT.md`) a human hand or the
`DISPLAY=:1` lane would be needed to drive a real platform file dialog. This is the row's own named
gap, and it remains a genuine gap rather than a closed one; the row cannot honestly read `PASSED`
while a full third of its own clause (Choose...) has never been driven by anyone at any point in this
project's history.

Screenshots: `reference/linux-progress/waveK-critic/prj1718-01-settings-open.png`,
`prj1718-02-typed-pin.png`, `prj1718-03-after-use-primary.png`, `prj1718-04-reopened.png`,
`prj1718-05-choose-clicked-no-picker.png`.

---

## 8. F-CHAT-33 — the MCP-warning (non-retryable) half of the OK-to-dismiss control

Ledger line 182, commit `83be7a14` (ancestor of HEAD). The wave-I builder's own re-drive covered
only the retryable turn-error banner live; its named gap: "I did not myself live-drive the
MCP-warning (non-retryable) half this pass... A fresh critic should reproduce a real
broken-`.mcp.json`-driven MCP warning live and confirm OK renders/works there too."

**Turn-error half — independently re-driven fresh, clean single-invocation drive (earlier
same-session leftover screenshots from this lane's own interrupted predecessor attempt were
internally inconsistent — one showed the *entire* transcript wiped by OK, not just the error row —
so rather than rely on them this pass redrove the whole sequence from a freshly-launched instance in
one uninterrupted `wayland-drive.sh` script, avoiding the relaunch-vs-KEEP race that produced that
inconsistency):** fresh scratch repo `/tmp/wfj-chat33err-repo`, `TILLER_ACP_PROGRAM` pointed at a
wrapper around `chat_fixture.py death-then-ok`, sidebar right-click → New Chat (the override-honoring
route), typed `WFJCE_ERR_MARKER_2`, Return. Result
(`reference/linux-progress/waveK-critic/chat33-01-turn-error-with-ok-button.png`): user bubble,
"partial" assistant chunk, red error banner "prompt failed: Incoming transport closed: ..." with both
"Restart agent" and "OK". Clicked OK: error banner gone, marker bubble and "partial" chunk both
**still present**, composer still reads "Agent offline — reconnecting when you send..." (OK does not
restart) — `reference/linux-progress/waveK-critic/chat33-02-after-ok-marker-and-partial-retained.png`.
Matches the row's own clause and the builder's claim exactly, on my own fresh binary and fresh drive.

**MCP-warning half — driven to its actual, structural conclusion rather than assumed reachable.**
Before attempting a live drive, `rust/crates/tiller_acp/src/mcp_config.rs`'s doc comment and a
dedicated `#[ignore]`d integration test
(`real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json`, `tiller_acp/src/lib.rs:2634`)
already document a prior sweep's exhaustive by-hand investigation of exactly this path, concluding it
is unreachable through two independent gates: (1) **Trust** — the first time any project's
`.mcp.json` names a server, the real agent marks it "Pending approval" and never attempts a
connection, and that approval lives in the user's own global `~/.claude.json`, which nothing in
Tiller's launch path populates; (2) **Channel** — even manually pre-approved, the real failure text
(`Failed to connect — ENOENT...`) arrives as ordinary `session/update` conversational content, never
on the child process's own stderr, which is the only channel `drain_stderr`/`looks_like_mcp_warning`
reads. I did not take this on trust (reading code is not evidence): I **re-ran the ignored test
myself**, fresh, with real network access and a real `npx @agentclientprotocol/claude-agent-acp@latest`
process spawned against a real broken `.mcp.json` (`{"mcpServers":{"broken-server":{"command":
"/definitely/missing/mcp-nonexistent-binary"...`), waited the full 10s past startup, and got:

```
cargo test --manifest-path rust/Cargo.toml -p tiller_acp --lib \
  real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json -- --ignored --nocapture
test tests::real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json ... ok
```

A genuinely executed, network-touching, real-agent-spawning test, green at current HEAD — confirming
`mcp_warnings()` stays empty for exactly the scenario a live UI drive would need. Reproducing the
*live UI* end of this would require either asking the agent to explicitly run `claude mcp list` (which
routes the failure text through ordinary chat content, not the `McpWarning` banner mechanism at all —
a different code path than the one this row's clause is about) or editing the real user's global
`~/.claude.json` to pre-approve the project — mutating shared state outside this repo and outside any
sandboxed lane, which I did not do and which none of this project's prior sweeps did either for this
same reason.

What I *did* independently confirm live is the **rendering mechanism itself**: re-ran
`an_mcp_warning_offers_ok_to_dismiss` fresh at current HEAD —
```
cargo test --manifest-path rust/Cargo.toml -p tiller_ui --lib offers_ok
test chat::tests::an_mcp_warning_offers_ok_to_dismiss ... ok
test chat::tests::a_retryable_turn_error_offers_ok_alongside_retry ... ok
```
This is legitimate UI-tier evidence under `EVIDENCE-STANDARD.md` (drawn via `TestAppContext`,
`cx.debug_bounds` hit-tests the real banner/button geometry, `cx.simulate_click` dispatches a real
click, then asserts the entry is actually removed) — not a data-model check. It proves: an
`Entry::Error{kind: McpWarning, retryable: false}` row, however it comes to exist, renders no Retry,
does render OK, and OK removes it. What it cannot prove, and what nothing in this project's history
has ever proven, is that the real agent+`.mcp.json` path can ever construct that row live.

**Verdict: half-proven**, and this pass narrows the gap to something sharper and more useful than
"nobody tried yet": the turn-error/retryable half is **PASSED** on my own fresh live drive; the
MCP-warning/non-retryable half's *rendering control* is proven by a real drawn-and-clicked test, but
the *live scenario* the row's clause implicitly assumes exists (a real broken `.mcp.json` producing a
real McpWarning banner through ordinary use) is **not reachable** by any means this pass could
exercise without mutating the real user's global Claude config — confirmed by my own fresh execution
of the real-agent integration test, not merely inherited from a code comment. A future pass could only
close this by either getting explicit sign-off to mutate `~/.claude.json` in a disposable way, or by
the agent's own behavior changing to write connection failures to stderr (exactly the signal the
ignored test is watching for and will go red on if it ever happens).

---

## Summary

| Row | Verdict | Notes |
| --- | --- | --- |
| CENTER-01 | PASSED | mandatory safety-gate exercised live: real host PID survived 5 worktree switches while `needs-input` |
| F-CORE-ACT-20 | PASSED | mandatory gap closed: real, installed GNOME `notification-daemon` (not a stub) on a private bus, real `Notify`/`GetServerInformation` handshake |
| F-SID-19 | PASSED | generalizes: a second root keybinding (`Ctrl+Shift+P`) also fires from the same zero-tab empty state |
| F-TERM-10 | PASSED | negative case confirmed: a genuinely idle bare shell's tab content still reloads on switch-away, fix did not overshoot |
| F-PRJ-13 | PASSED | exact original fixture geometry reproduced live (decoy row under Reset's own pixel position); also a real red→green test at the exact commit boundary |
| F-PRJ-17 | PASSED | "Use Primary" clicked live, persists across reopen and in the on-disk DB |
| F-PRJ-18 | half-proven | typed-path leg PASSED live with DB cross-check; "Choose..." native picker leg is `NOT EXERCISED` — no portal in this lane |
| F-CHAT-33 | half-proven | turn-error/retryable half PASSED live; MCP-warning/non-retryable half's UI control is proven by a real drawn+click test, but the live scenario that would produce it is confirmed unreachable without mutating the user's global Claude config |

Six of eight rows close clean. The remaining two are not evasions: `F-PRJ-18`'s gap is an
environment ceiling (no Wayland portal in this lane, stated plainly in `WAYLAND-LANE.md`), and
`F-CHAT-33`'s gap is a structural property of the real agent this project ships against, confirmed by
this pass's own fresh execution of the relevant tests rather than inherited from a comment.


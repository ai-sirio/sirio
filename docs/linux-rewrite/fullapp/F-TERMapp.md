# F-TERM (app) — finish-line critic pass

Section: "Terminals and agents — app" (11 rows, bare `F-TERM-NN` ids). Distinct from the
package-tier `F-TERM-*` rows elsewhere in the ledger — not covered here.

Critic: fresh, independent pass. Label `sw12term*` (several sub-labels per invocation, since
several UI-coordinate-finding rounds and one recovered self-inflicted harness bug forced
restarts — see "Traps hit" below). Fixtures: `/dev/shm/sw12term-fixture` and
`/dev/shm/sw12term-fixture2`, throwaway git repos. Binary: `/dev/shm/tt/debug/tiller` (warm
build, not rebuilt). All screenshots referenced below live under `/dev/shm/sweep-12-F-TERMapp/`
(not committed — ephemeral scratch, per the brief).

Every row was independently re-driven against the live app this pass. The ledger's existing
PASSED verdicts were treated as claims to verify, not as truth — **five of the eleven rows are
downgraded below**, three of them to a confirmed **FAILED — defective**, with source citations
and reproductions for each.

## Verdict table

| row id | ledger said | my verdict | evidence |
|---|---|---|---|
| F-TERM-01 | PASSED | **PASSED** | Live: selected worktree, clicked New Terminal, a real shell (neofetch banner + `❯`-style prompt) rendered immediately. Typed `echo KEYBOARD_ALIVE_MARK_9f3` + Enter, confirmed on screen AND independently via a raw `panel.read` socket call — log line `KEYBOARD-CHECK: MARKER_FOUND`. `/dev/shm/sweep-12-F-TERMapp/main2/{03,04,05}-*.png`, `main2.log`. |
| F-TERM-02 | PASSED | **half-proven** | Live: confirmed the *other* empty state (`group.id==0`, "No Terminals" / single "New Terminal" button) is a different surface from this row's target. Live: created a genuine second pane group via the tab's "Move to New Pane" context-menu item — confirmed it renders as an independent, fully live pane (its own PTY, own neofetch banner, own prompt), proving the underlying group-creation mechanism is real. Could **not**, within budget, pixel-locate the second tab-strip precisely enough to blind-click "Move to Pane 0" and vacate it live (harness/coordinate limitation — see Traps). Corroborated instead by running `tests::drawn_detached_pane_group_offers_the_real_empty_prompt` fresh this pass (`cargo test -p tiller --bin tiller`, **1 passed**) — a full GPUI render+simulated-click test that performs exactly this sequence and asserts both `terminal-new` ("New Terminal") and `terminal-new-command` ("New…") are drawn, then simulates a click on New Terminal and confirms it lands in the correct group. The **"New…" action's own resulting surface was never actually invoked/observed** by this pass or by the test. `/dev/shm/sweep-12-F-TERMapp/phase{4,5,6,7}/*.png`. |
| F-TERM-03 | PASSED | **half-proven — disagree with ledger** | Freshly live-drove all three *exit* legs via `exec sh -c 'exit 0'`, `exec sh -c 'exit 7'`, and `exec sleep 100` + host-side `kill -9`: each produced a correct, distinct tab badge (`✓ exit` / `! exit 7` / `! signal 9`) and bottom-left status text (`Process exited successfully` / `Process exited with status 7` / `Process terminated by signal 9`). `/dev/shm/sweep-12-F-TERMapp/cont/{02,03,05}-*.png`. **But**: traced the "running" leg through source (`tiller_terminal/src/lib.rs:1977-1985`) — the status overlay only ever renders `when_some(self.exit_status...)`; nothing is drawn while a command is merely executing. The tab's own activity dot is agent-identity-gated (`AgentCatalog`/Layers A-D per CLAUDE.md), not "a child process is alive" — confirmed live: a `sleep 20` run to completion never changed the tab from a plain hollow "○", even while genuinely running (`/dev/shm/sweep-12-F-TERMapp/main2/06-05-running-state.png`). So 3 of 4 VERIFY states are solid; the "running" state has no positive visual distinguishing it from "nothing has run yet" for an ordinary command — only the *absence* of a pill. |
| F-TERM-04 | PASSED | **PASSED** | Live: drag-selected terminal text, context-menu Copy, then context-menu Paste into the terminal — a real multi-line block of text flowed through the clipboard and was genuinely typed into the shell (visible as a cascade of `bash: ... File o directory non esistente` errors, which is itself proof the paste was real, not a no-op — my selection was just wider than intended). `/dev/shm/sweep-12-F-TERMapp/main2/13-12-after-copy-paste.png`. |
| F-TERM-05 | PASSED | **FAILED — defective** | Copy Context: PASSED live (Paste after Copy Context inserted real surrounding multi-line text — `cont/07-06-copy-context-pasted.png`). **Set Title: confirmed broken**, both live and in source. Live: clicking "Set Title" instantly renamed the tab to `Terminal terminal-0` with **no text-entry prompt of any kind**; my subsequent typing landed as literal shell input instead (`cont/08-07-set-title-opened.png`, `cont/09-08-after-set-title.png`). Source: `rust/crates/tiller/src/main.rs` `set_terminal_title()` — `tab.title = format!("Terminal {terminal_id}")`, with the code's own comment: *"The Linux context event carries identity, not text. Use that stable identity as the app-owned title **until a text-entry prompt is added**."* This is a self-documented incomplete stub, not a working custom-title feature — the row's "confirm... title changes [to what was typed]" clause cannot be satisfied because there is no way to type a title at all. Clear Terminal: inconclusive in my run (see Not Exercised below) — not the basis for this verdict. One of three required sub-actions is source-confirmed non-functional, so the row fails as a whole. |
| F-TERM-06 | PASSED | **PASSED** | Live: Copy Pane ID → Paste produced `pane-0`; Copy Terminal ID → Paste produced `terminal-0`. Both nonempty, both distinct, both real clipboard round trips. `/dev/shm/sweep-12-F-TERMapp/cont/{11,12}-*.png`. |
| F-TERM-07 | PASSED | **PASSED** | Live: the tab-strip "+" menu offered exactly New Terminal, Changes, New Browser, **Claude Code, Codex, OpenCode, Pi, Oh-My-Pi**, Split Claude Code, New Chat. Chose Claude Code — a real `claude` process launched, progressed through its actual first-run trust prompt ("Quick safety check... 1. Yes, I trust this folder") to the full "Welcome back Enzo!" banner. Sidebar dot and "Activity · 1 running" appeared immediately. `/dev/shm/sweep-12-F-TERMapp/agent/02-01-plus-menu.png`, `agent2/02-01-trust-prompt.png`. |
| F-TERM-08 | PASSED | **FAILED — defective — loud disagreement with ledger** | Live-reproduced cleanly: started `sleep 300` in a plain (non-agent) terminal, confirmed it actively running on screen, right-clicked → "Close Terminal…". **The pane closed instantly, with zero confirmation of any kind** — no prompt, no "Close Anyway" banner — straight back to the "No Terminals" empty state in one click. `/dev/shm/sweep-12-F-TERMapp/phase8/02-01-menu-open.png` (menu open, `sleep 300` visibly running) → `phase8/03-02-close-confirm-dialog.png` (instantly empty). Traced to source: `request_close_terminal_at` (`rust/crates/tiller/src/main.rs:4817`) gates the confirm banner entirely on `pane_close_needs_confirmation(status)`, and `status` comes from `pane_activity_status` — the **agent-recognition** activity status (`AgentCatalog`-based Layers A-D), not "does this pane have a live child process." A generic shell command that isn't a recognized agent CLI can never reach `ActivityStatus::Running/NeedsInput/Error`, so the confirmation gate is permanently closed for ordinary terminal use. This directly contradicts the row's VERIFY ("Start a command that remains running... confirm the prompt, accept it") and the ledger's own current evidence text, which claims exactly the opposite ("confirm affordance appeared, accepted"). I could not find any confound in my repro (single deterministic click, no race) — this is a genuine, source-confirmed regression. |
| F-TERM-09 | PASSED | **half-proven** | Live: while Claude Code was starting and then actively processing a real prompt, observed a solid/working status indicator, a live "Sautéed for Ns" elapsed-time readout, growing context/token counters, and "Activity · 1 running" — a real, observed working state (`agent3/03-02-sent-working.png`). Did **not** cleanly close the loop back to idle within budget: the response stayed stuck mid-turn for the rest of my observation window under heavy host contention (6+ concurrent `tiller`/`claude` processes measured via `pgrep`/`free` during this pass) rather than completing — recorded as environment load, not a defect (see Traps). The working→idle *transition* itself is separately corroborated by a same-host screenshot I directly reviewed today (`reference/linux-progress/wf-sweep2/f-term-10-marked-before-switch.png`, showing a solid "Claude Code ●" next to hollow "Terminal ○" tabs in the same frame) and by the documented Layer A-D model in CLAUDE.md, but I did not personally close a single continuous before/after capture. |
| F-TERM-10 | PASSED | **PASSED** | Live: marked and ran `echo MARK-TERM10-BEFORE && sleep 300` in project A's terminal (confirmed running), switched to an independent second project B (`ctl workspace.current` confirmed the backend selection genuinely left A), then switched back to A. The pane still showed the identical `MARK-TERM10-BEFORE` text, no fresh shell banner, no exit pill — same live session, `sleep 300` never interrupted. `/dev/shm/sweep-12-F-TERMapp/term10/{02,04}-*.png`. Side note, not scored: the pane's cosmetic "in bash at HH:MM:SS" creation-time badge (a value computed once and otherwise static — confirmed in source, `shell_breadcrumb()`) *did* change across the round trip, consistent with the front-end view remounting on reselect (per CLAUDE.md's `openWorktreeIds` mount/unmount architecture) while the backend PTY is never torn down — cosmetic, not a state loss. |
| F-TERM-11 | PASSED | **PASSED** | Live, genuinely cold boot (fresh label, no project ever added): central surface read "No worktree selected" / "Add a project, then select a worktree." — captured before any `ctl` call reconstructed state. `/dev/shm/sweep-12-F-TERMapp/main2/02-01-boot-no-worktree.png`. |

Score: **6 PASSED** (01, 04, 06, 07, 10, 11), **3 half-proven** (02, 03, 09), **2 FAILED —
defective** (05, 08). The ledger currently carries all 11 as PASSED.

## Defects (reproducible, with repro)

1. **F-TERM-08 — Close Terminal… never confirms for an ordinary running command (most severe).**
   Repro: open any plain terminal, run something long-lived (`sleep 300`), right-click → Close
   Terminal…. The pane closes immediately, no prompt. Root cause in
   `rust/crates/tiller/src/main.rs::request_close_terminal_at`: the confirm gate reads
   `pane_activity_status`, which is agent-CLI-recognition-based (`AgentCatalog`), not PTY
   liveness. Screenshots: `/dev/shm/sweep-12-F-TERMapp/phase8/01-menu-open.png` →
   `02-close-confirm-dialog.png`.

2. **F-TERM-05 — "Set Title" has no text-entry prompt; it's a fixed-value stub.** Repro: right-click
   a terminal → Set Title. The tab is immediately renamed to `Terminal <terminal_id>`; no dialog
   or inline field ever captures keyboard focus, so a user cannot set any title at all. Source:
   `rust/crates/tiller/src/main.rs::set_terminal_title`, self-documented as incomplete ("until a
   text-entry prompt is added"). Screenshots: `/dev/shm/sweep-12-F-TERMapp/cont/08-07-set-title-opened.png`,
   `09-08-after-set-title.png`.

3. **F-TERM-03 — no "running" indicator exists for a generic (non-agent) command.** Repro: run
   any long command (`sleep 20`) in a plain terminal and watch the tab — it never changes from
   idle while genuinely executing; only Success/Code(N)/Signal(N) ever render, and only after
   exit. Source: `rust/crates/tiller_terminal/src/lib.rs:1977` (`when_some(self.exit_status...)`,
   nothing drawn while `None`) plus the agent-gated activity dot
   (`tiller_activity::content::detect_content_status` requires a known `agent_id`). Not
   necessarily a regression from the *row's* stated intent if the four states are read as merely
   "distinguishable outcomes" (no pill vs. a pill does distinguish running from done) — flagged as
   a real, code-confirmed gap either way.

## Not exercised / could not close

- **F-TERM-02's "New…" action resulting surface.** Only "New Terminal" was ever actually clicked
  (live, via the app's own GPUI test); "New…" (which opens the New-Tab palette per
  `TerminalPromptAction::NewTerminalWithCommand`) was never invoked by this pass or found in the
  existing test suite with a followed-through click. Reason: ran out of budget after the
  coordinate-finding cost of F-TERM-02's live half (see Traps below).
- **F-TERM-05's Clear Terminal clause**, cleanly isolated. My live attempt followed an oversized
  Copy-Context paste that left the pane in a messy multi-line-error state; after Clear Terminal
  some prior lines still appeared to be visible in the capture
  (`/dev/shm/sweep-12-F-TERMapp/cont/10-09-after-clear-terminal.png`). I can't rule out this being
  a scroll-position artifact from the preceding confound rather than a real partial-clear defect,
  so I did not fold it into the F-TERM-05 verdict's reasoning — it rests entirely on the
  source-confirmed Set Title stub instead. Worth a clean, isolated re-test (plain terminal, `seq
  100`, Clear Terminal, nothing else beforehand).
- **F-TERM-09's live idle transition**, my own continuous before/after. Got a solid "working"
  capture but the turn never settled within my observation window under heavy concurrent host
  load (verified via `pgrep -c -f "debug/tiller"` = 4-6 and `free -h` showing 4-5 GiB free
  throughout this pass) — an environment/timing issue, not a demonstrated defect.

## Traps hit this pass (harness lessons, not app defects)

- **Self-inflicted `pgrep -f` false-positive nearly wrecked the whole first main pass.**
  `pgrep -f "sleep 100"` matches against full argv — and the wayland-drive.sh action script's own
  argv literally contains the substring `sleep 100` (from `type "exec sleep 100"` embedded in the
  action text), so it can match the *driver script itself*, not the intended PTY child. A
  `kill -9` on that match cascaded into a dead sway socket and a fully wrecked run (see
  `/dev/shm/sweep-12-F-TERMapp/main.log`). Fixed by switching to `pgrep -x sleep -n` (exact comm
  match, newest). Anyone scripting a signal test the same way should use `-x`, never `-f`, for a
  short, common command name.
- **Locating a second pane group's own tab-strip coordinates cost far more than expected.**
  "Move to New Pane" genuinely creates an independent, live pane group (confirmed), but its tab
  button did not respond to right-clicks at the naive mirrored position, and screenshots alone
  could not resolve why (crops showed a "+" but no visible tab label for the new group). Several
  exploratory invocations were spent on this before falling back to the GPUI test as
  corroboration for F-TERM-02's second half — a worthwhile trade against the section's overall
  time budget, but it should be flagged for whoever next needs to drive multi-pane-group UI live.
- **`/dev/shm` briefly sat at 97% full** at the start of this pass (other agents' concurrent
  runs) and memory dropped as low as ~4.1 GiB free mid-pass; every `wayland-drive.sh` invocation
  in this report was sized and paced around that, and no single invocation left artefacts beyond
  its own `/dev/shm/sweep-12-F-TERMapp/` subtree.

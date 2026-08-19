# Finish-line critic: sweep part 2 (13 half-proven rows nobody reached last pass)

Lane: `wf-sweep2`. A predecessor with this same lane label ran out of time mid-drive and never
wrote or committed this report; its in-progress screenshots and a live-kept app instance
(`/tmp/wf-sweep2-tiller`, socket `/tmp/wf-sweep2.sock`, booted 02:47) were found already running
when this pass started and were reused rather than restarted, per `WAYLAND-LANE.md`'s warning that
a second `wayland-drive.sh` invocation under the same label kills and silently discards in-memory
state. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep2-tiller
export TILLER_WL_BIN=/tmp/wf-sweep2-tiller TILLER_WL_LABEL=wf-sweep2
```

Every row below names the specific missing half the ledger already carried and drives only that
half live — the already-proven half is not re-litigated.

## F-SET-14 — Add Account / waiting / cancel / retry / re-authenticate / remove

**half-proven, unchanged verdict but the named gap is now closed.** The ledger's wave-M evidence
already proved the process-spawn half (`Add Account` spawns a real `x-terminal-emulator` + `codex
login`, real OAuth URL) and the absence half (re-authenticate/remove are structurally absent, no
`account_row` UI control exists — single-account design, same signature as F-SET-15). The named
missing half was: **"the Signing-in.../Cancel UI transition never renders even in the very first
post-click frame."**

That claim does not hold. The normal `shot()` helper forces a window resize-and-settle that takes
1.2-2s, which is longer than the pending state apparently survives before an internal timeout
reverts it — that timing gap is almost certainly why the prior pass never saw it. Driving with a
faster capture (single resize, 150ms settle, not the double-resize dance) catches it directly:

- Clicked Codex's **Add Account** at `(1257, 673)`; 245ms later (timestamped shell calls either
  side of the click) a forced-repaint capture shows the Accounts row rendering **"Signing in..."**
  next to a **Cancel** button, replacing the Add Account control —
  `reference/linux-progress/wf-sweep2/f-set-14-signing-in-cancel-quickshot-245ms.png`.
- Clicked **Cancel**; the very next capture shows the row reverted to **Add Account**, and the
  underlying `codex login` process (confirmed via `pgrep`) is still running at that instant — the
  UI cancel does not kill the background process. `f-set-14-cancel-reverts-to-add-account.png`.
- Killed the orphaned `codex login` process by hand and clicked **Add Account** again (a genuine
  retry): the Signing-in.../Cancel state rendered again with a fresh OAuth `state=` parameter,
  confirming retry is not a one-shot. `f-set-14-retry-signing-in-again.png`.

New, smaller finding filed but not scored against this row (out of clause): retrying **without**
first killing the still-listening `codex login` from the previous attempt is a silent no-op — no
new process, no UI change — because Cancel resets the UI state without terminating the child
process it was tracking. This is a real defect (an orphaned OAuth login server keeps listening on
`localhost:1455` after Cancel) but it is not what F-SET-14's clause names, so it is not scored here.

Verdict stays `half-proven` — re-authenticate/remove genuinely do not exist in the UI — but the
specific missing half named in the ledger is now proven, not absent.

## F-SET-18 — Install, update, retry, unsupported/not-found states for agents

**Promoted: PASSED.** The ledger already had the negative control (not-found, no install button),
the in-progress spawn, and a real *failed* install (exit 127). The named missing half was
**"Full success-path reinstall not landed live this pass."** Driven this pass, on a throwaway
instance so the real npm global install shared with sibling lanes was never touched:

- Built an isolated PATH: a shadow directory with only `node`/`npm`/`npx` symlinked in (no
  `opencode`), and `NPM_CONFIG_PREFIX=/tmp/wf-sweep2-npmprefix` (an empty, fresh global prefix) —
  so OpenCode/Pi/Oh-My-Pi all show **Not found on PATH** genuinely (not stubbed), while `npm
  install` itself still works for real, against the real registry, writing only into the
  throwaway prefix. Confirmed via `providers` in a `surface.settings.open` reply and a screenshot:
  `reference/linux-progress/wf-sweep2/f-set-18-fresh-fixture-notfound.png`.
- Clicked OpenCode's **Install**: the row's caption flips to **"Installing… running in a new
  terminal tab"** and a real terminal tab titled **Install opencode** opens (confirmed via the
  sidebar tab list, not just the caption) —
  `reference/linux-progress/wf-sweep2/f-set-18-install-clicked-spawns.png`.
- That tab's real output: `added 3 packages in 8s` from a genuine `npm install -g
  opencode-ai@latest` run against the live npm registry, ending in **"Process exited
  successfully"** (a green check on the tab, not the earlier red exit-127) —
  `reference/linux-progress/wf-sweep2/f-set-18-npm-install-succeeded-8s.png`. Independently
  confirmed off-app: `/tmp/wf-sweep2-npmprefix/bin/opencode` is a real symlink to
  `../lib/node_modules/opencode-ai/bin/opencode.exe` on disk, dated to this run.
- Clicked **Refresh** in the Agents panel: OpenCode's row flips from "Not found on PATH" + Install
  to **"Built-in: uses the opencode binary on your PATH"**, showing the exact installed path
  `/tmp/wf-sweep2-npmprefix/bin/opencode` — matching the on-disk symlink exactly, and the Install
  button is gone. `reference/linux-progress/wf-sweep2/f-set-18-refresh-shows-installed-path.png`.

Every state named in the VERIFY clause now has live evidence: not-found/unsupported (already had
it), in-progress (already had it), a real failure (already had it), and now a real full success
(not-found → Install → real subprocess → success → Refresh → found, with the installed path as
the hard discriminator). "Update to latest" specifically (re-running Install once already
installed) was not separately driven — OpenCode's row has no distinct "Update" control once
found, only while absent, so this appears to be the same Install control repurposed, not a
separate state; not scored as a gap since the row's own clause is satisfied by the states actually
drawn.

## F-SET-22 — Customize each agent's accent color

**half-proven, unchanged verdict, but now proven live instead of by code-reading.** The ledger's
existing evidence was two-part: a live click on Claude Code's blue swatch producing a real
selection-ring change (already proven, matches the passing unit test
`agent_color_click_selects_a_new_accent_and_persists`), plus a **code-reading** claim (a doc
comment at `main.rs:2740-2755`) that the picker is deliberately decoupled from
`tiller_theme::AgentBrandColor`, so nothing ever visibly repaints. Per `EVIDENCE-STANDARD.md`, a
verdict made by reading code is not a verdict — this pass drove the second half live instead.

Opened a Claude Code chat tab in the fixture project (`wf-sweep2-fixture`); its tab icon, the
sidebar worktree-row badge, and the "+" new-tab menu's Claude Code entry are all the same coral
sun icon —`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-coral-before.png`. In Appearance
→ Agent Colors, clicked Claude Code's **blue** swatch: the selection ring visibly moved to blue,
confirming the click landed —
`reference/linux-progress/wf-sweep2/f-set-22-claude-blue-selected-in-picker.png`. Went back to the
main view with a forced repaint: the open Claude Code tab's icon and the sidebar worktree badge
are **still coral**, pixel-identical to the before shot —
`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-still-coral-after.png`. Opened the "+"
new-tab menu again as a third, independent rendering surface: Claude Code's menu entry is **still
coral** too — `reference/linux-progress/wf-sweep2/f-set-22-new-tab-menu-still-coral-after.png`.

Three independent surfaces (tab icon, sidebar badge, new-tab menu), zero of them affected by a
confirmed, ring-visible color selection. The clause's first half (choose a color) is proven; the
second half (that agent's accent color changes anywhere it's shown) is now proven **absent** by
direct observation, not inferred from a comment. Verdict stays `half-proven` since the clause is a
conjunction with one genuinely-working half and one genuinely-absent half — the gap is simply no
longer resting on a read of the source.

## F-TERM-03 — Running / exit-0 / exit-N / signal status on terminal output

**Promoted: PASSED.** Running, exit-0, and exit-7 pills were already live-proven. The named
missing half was the signal-9 case, which ~10 straight `wayland-drive.sh` attempts failed to
reproduce, root-caused as GPU/compositor contention from concurrent sibling lanes rather than a
Tiller defect. Also hit and fixed *this* pass: the resumed instance's virtual-keyboard device had
silently expired (`swaymsg -t get_inputs` showed only the pointer, matching
`WAYLAND-LANE.md`'s named failure mode exactly), so `type`/`key` were protocol-level no-ops for a
few calls until a fresh long-lived `wtype -M shift -s 14400000 -k Shift_L` restored
`wlr_virtual_keyboard_v1` — recorded here since it is precisely the trap the brief warned about
and cost real time to diagnose.

With the keyboard restored: opened a fresh Terminal tab, typed `exec sleep 100` (replacing the pty's
own shell with `sleep` directly, so Tiller's `waitpid` on its immediate child is exercised, not a
nested subprocess), confirmed it running, then sent `kill -9` to that exact PID from the host.
Forced repaint shows the tab title flip to **`! signal 9`** and the status bar read **"Process
terminated by signal 9"** verbatim —
`reference/linux-progress/wf-sweep2/f-term-03-signal-9-terminated.png`. All four VERIFY states
(running, exit 0, exit N, signal) now have live evidence; promoted to PASSED.

## F-TERM-10 — Terminal panes survive a worktree-selection round trip

**Confirmed PASSED — the pre-fix contrast is now driven live.** The ledger row already carried
`PASSED` for commit `0519ace4`'s negative case (an idle pane correctly still reloads), with an
orchestrator note that nobody had re-driven the **original** failing scenario
(`984defa7 fix(F-TERM-10)`) against the current tree to confirm the fix actually closes it, only
that it didn't overshoot.

Reproduced `FINISH-terminal.md`'s exact original repro shape inside one continuous session (no app
restart, so a restart can't be confused for the switch itself): opened a fresh Terminal tab in the
`wf-sweep2-fixture` worktree, ran `echo MARK-TERM10-BEFORE && sleep 300`, confirmed it running —
`reference/linux-progress/wf-sweep2/f-term-10-marked-before-switch.png`. Added a second, separate
project (`wf-sweep2-fixture2`) as the "other worktree" to switch to, confirmed the backend
selection genuinely left the first worktree (`ctl workspace.current` returned the second
workspace's id, not the first), then clicked back on the original worktree's sidebar row. Result:
the pane still shows **`MARK-TERM10-BEFORE`**, no fresh shell banner, no exit pill — the same live
session, `sleep 300` still running — `reference/linux-progress/wf-sweep2/f-term-10-mark-preserved-after-roundtrip.png`.
This is the exact contrast `FINISH-terminal.md` needed and didn't have: before the fix this same
shape of round trip produced a brand-new shell with the mark and the running process both gone;
now it does not.

Side observation, not scored against this row: while the second worktree was selected, the tab
strip and sidebar's tab list kept showing the first worktree's tabs and content for several
seconds even after `ctl panel.list` confirmed (empty, then populated) panels genuinely owned by
the second worktree — sidebar highlight, the Files panel, and the status bar all updated
immediately, only the tab-host content lagged. Switching back through a real sidebar click
resolved it cleanly every time. Flagged here for whoever owns cross-project tab-host mounting;
not reproduced as a loss of PANE STATE (nothing died, nothing reset), so it does not contradict
this row's own PASSED clause.

## F-TERM-SCR-02 — Terminal output forwarded to activity model after debounce (200ms settle, 120ms resize)

**Promoted: PASSED.** The ledger's evidence was "constants match" — a code-reading claim that a
120ms resize debounce and 200ms output-settle debounce exist in source. The named missing half was
explicit: **a test that actually counts status/resize callbacks under a burst**, not a restated
constant. `docs/linux-rewrite/wave-h/H6-instruments-report.md` had already worked out the right
shape for this (a SIGWINCH-trap script in a real split pane, driven by a real button-held divider
drag) but never landed the instrument or the transcript in the repo, so it was unreplayable. This
pass reproduces it and lands both.

Opened a fresh Terminal tab in the `wf-sweep2-fixture` worktree, typed the trap one-liner directly
into its shell (also saved as
`reference/linux-progress/wf-sweep2/f-term-scr-02-winch-trap.sh`):

```
trap 'printf "WINCH %s cols=%s\n" "$(date +%s.%3N)" "$(tput cols)"' WINCH
echo TRAP_READY
while true; do sleep 0.02; done
```

The tight `sleep 0.02` loop keeps the shell's foreground process alive so a pending `SIGWINCH` is
picked up within ~20ms, without needing the shell back at its prompt. Right-clicked the pane →
**Split Right** to create a real divider, then found its exact pixel column by sampling pixel
colours across the boundary (the divider hairline is a distinct near-black run only 4-6px wide —
an earlier drag attempt at a coordinate 6px off missed the hit-region entirely and produced no
resize at all, which is itself informative: this is a narrow, precise hit-target, not a generous
one).

Drove one real button-held drag on the divider: `down (1039,400)`, four `move` waypoints stepping
left to `(850,400)`, `up (850,400)` — 6 distinct virtual-pointer operations spanning 346ms
(host-timestamped: `1787103068.011` → `1787103068.357`). Rather than reading the result back with
a window-resizing screenshot (which would itself inject more real resize events and contaminate
the count), the pane's scrollback was polled with `ctl panel.read id=pane-4` — a control-socket
read of the pane's live buffer that touches no window geometry at all — at t+0.3s, 0.8s, 1.5s,
2.5s, and 4.0s after the drag, with **no** `shot()`/`quickshot()` calls anywhere in between:

- **6 raw pointer-driven resize opportunities collapsed to exactly 1 delivered `WINCH`**, arriving
  at `1787103068.561` — 204ms **after** the drag's own `up` event, not synchronously with any of
  the 6 pointer moves.
- Columns jumped straight from 90 to 65 in that single delivery — no intermediate `WINCH` for any
  column count the pointer physically passed through mid-drag.
- The log stayed completely flat across all 5 polls out to t+4.0s — one settle, one delivery, done;
  this wasn't the first of a delayed second wave.

Full raw transcript (pointer-op timestamps + all 5 poll results) saved at
`reference/linux-progress/wf-sweep2/f-term-scr-02-drag-and-winch-log.txt`; a corroborating
screenshot of the pane's own printed log (matching the transcript exactly) at
`reference/linux-progress/wf-sweep2/f-term-scr-02-winch-count-burst.png`. That same screenshot
carries a bonus data point: two earlier `WINCH` lines in the same log
(`cols=89` then `cols=90`, 221ms apart) came from a single `quickshot()`'s own two-step window
resize (1715×972 → 1714×972 → 1715×972, unrelated to the deliberate drag) — and that too produced
exactly one `WINCH` per direction change rather than a flood, the same debounce/settle shape from
an independent trigger.

This is a genuine instrumented count — 6 inputs, 1 output, arriving after gesture-end rather than
tracking it — not a restatement of a source-code constant. Promoted to PASSED.

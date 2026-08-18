# Critic pass on wave O — five owed items, judged live

Fresh critic, did not build any of wave O. Host: the x86 desktop described in
`ENVIRONMENT.md`'s 2026-08-18 section (12 cores, AMD GPU, COSMIC/Wayland). Lane: the nested
Wayland lane (`Scripts/wayland-drive.sh`), `TILLER_WL_LABEL=waveo` (and `waveofix` for the
fixture-only sub-test on F-CHAT-23's dismiss conjunct). Binary pinned once and reused for every
drive in this pass:

```
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm, ~15s, same 2 pre-existing warnings
cp rust/target/debug/tiller /tmp/waveo-tiller
export TILLER_WL_BIN=/tmp/waveo-tiller
```

Judged strictly from each row's VERIFY clause in `01-inventory-app.md` / `02-inventory-packages.md`
(quoted below per row), not from the wave-O commit messages. Screenshots referenced below are
committed under `reference/linux-progress/waveO-critic/`.

Two harness notes worth recording for the next critic:

- **`TILLER_WL_KEEP=1` alone does not start the virtual pointer.** `wayland-drive.sh` only builds
  the persistent pointer client lazily, the first time the action block calls a pointer verb. A
  kept session whose bootstrap block used only `ctl`/`shot` has no `/tmp/<label>-input/` FIFO at
  all, and a raw write to the (nonexistent) FIFO from a later shell blocks forever with no error.
  Fix: always issue one throwaway `click` in the bootstrap block (e.g. `click 5 5`) so the pointer
  client exists before you leave the session running for follow-up calls.
- **`ctl`'s param encoding cannot carry a space.** `ctl()` joins all args with `IFS` and the Python
  side re-splits on whitespace, so `text="hello world"` silently becomes `text=hello` plus a
  dropped `world` token even when quoted at the call site — this is why every existing recipe in
  this repo uses `snake_case`/`No_spaces` chat markers. For real multi-sentence prompts (needed
  here to steer Claude into specific tool-call shapes), I bypassed the DSL and spoke to the control
  socket directly with a small Python script that preserves argv boundaries
  (`/tmp/.../wctl.py <method> k=v k2="v with spaces"`), reusing the same kept session's
  `$SOCK`/`$WAYLAND_DISPLAY`/`$SWAYSOCK`. Recorded here so the next agent doesn't lose an hour to
  the same silent truncation.

---

## 1. F-TERM-PTY-06 — PASSED

VERIFY (02-inventory-packages.md:86): *"Drop files onto a live agent shell and confirm the agent
receives the exact path text without an implicit Enter."* Ledger row's fuller clause (from
STATE.md's brief) adds the focus-return half the previous critic found broken.

Drive: real compositor-delivered XDND via `Scripts/xdnd-source` (`xdnd` action), not GPUI's
in-process simulated drag. Discriminator, exactly as instructed — type a marker, prove focus can be
moved away with a positive control, drop, type a second marker, read where each landed:

1. Clicked the Terminal pane, typed `QMARK1` (proves the pane is focused and takes keys).
2. Clicked the sidebar Filter field at its real position (`(150, 80)` — **not** `(325, 116)`, which
   in this build's layout lands below the field and silently missed it on my first attempt,
   producing a false-looking "focus never left" result. Corrected after inspecting a crop of the
   sidebar region).
3. Typed `WFILTER` — positive control: landed in the Filter box
   (`reference/linux-progress/waveO-critic/pty06-02-marker-and-filter-control.png`), proving focus
   really left the terminal and the discriminator is live, not a no-op.
4. Cleared the filter with backspaces.
5. `xdnd 6 6 700 400 /tmp/waveo-drop-a.txt /tmp/waveo-drop-b.txt` — real `wl_data_device` drag,
   `zwlr_layer_shell_v1` overlay source, dropped onto the live terminal pane.
6. Typed `ZMARK2`.

Result (`pty06-03-after-real-xdnd-drop.png`, `pty06-04-marker-after-drop-lands-in-terminal.png`):
the prompt line reads `QMARK1'/tmp/waveo-drop-a.txt' '/tmp/waveo-drop-b.txt'ZMARK2` — both paths
landed shell-quoted and space-joined at the cursor with **no newline run** (prompt never executed),
and `ZMARK2`, typed immediately after the drop with no further click, appended directly onto the
same terminal line rather than landing in the Filter field the positive control had just proven
reachable. Focus returned to the terminal.

**Flaky instrument, not a flaky feature — recorded so the next critic isn't fooled by it.** The
compositor-level drag self-cancels intermittently in this environment: two attempts in this pass
got `DRAG_STARTED` → `CANCELLED` with no `TARGET`/`DROP_PERFORMED` at all (self-offer race, the
known issue `xdnd-source`'s own code comments describe), and one attempt got a full
`DROP_PERFORMED` immediately followed by `FAIL: timed out after 15s waiting for dnd_finished` at
**delay-ms 0** — previously this async-pipe race was only documented above ~350ms of injected
delay. When that happened, the drop was genuinely lost (terminal unchanged) and the *next* typed
marker went to the Filter field, not the terminal, which would misread as "focus never returns" if
taken as the only sample. I retried in a loop (`for i in 1..5`) until a run produced
`TARGET`/`ACTION`/`SEND`/`DROP_PERFORMED`/`FINISHED` end to end, then judged that run. Verdict:
**PASSED on a successful drop**; the harness's own self-cancel/race is a lane property (documented
in `WAYLAND-LANE.md`'s xdnd section already), not something this row's clause is about.

## 2. SEAM-sidebar-identity — PASSED

Clause (STATE.md's OWED section): *"agent identity must reach the TAB row live for a pane
identified AFTER spawn through Layer B (the OSC title path), not only for a pane Tiller launched
itself."*

Drive: opened the plain `Terminal` tab that Tiller itself spawned as an ordinary shell (not an
agent — before this test it showed the generic terminal icon, `seam-01-before-osc-title.png`).
Clicked into it and used the `title` action (real `printf '\033]0;...\007'` typed into the live
shell and executed) to set an OSC 0 title containing the literal word `codex`
(`title codex ready for review`), which `identify_agent_from_title`
(`rust/crates/tiller_activity/src/title.rs`) recognizes via its word-boundary matcher — this is
genuinely Layer B, a title arriving well after the pane was created, not a spawn-time assignment.

Result (`seam-02-after-osc-title-codex.png`, cropped in `seam-03-sidebar-tab-row-crop.png` and
`seam-04-tabbar-crop.png`): **both** the sidebar's `Terminal` tab row under the worktree **and**
the top tab-bar's `Terminal` tab switched live from the plain grey terminal glyph to Codex's blue
pinwheel icon (`AgentBrandColor::Codex` = `0x0A84FF`), matching the same icon shown next to "Codex
logged out" in the status bar elsewhere in this build. This is exactly the seam the two commits
(`01ec3915`, `4fd069cf`) targeted: `tab_agent_mark`'s pane-id fallback resolving both icon and id
live on every sync, not only from `OpenTab::agent_id` set at spawn.

## 3. F-CHAT-22 — PASSED (both conjuncts)

VERIFY (01-inventory-app.md:180): *"In a multi-step transcript, click a WorkGroup and a collapsed
older-turn row, and confirm their contents unfold/fold."*

Drove three real turns against the installed `claude` CLI over ACP (worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, surface `default-chat`), via direct socket
calls (see harness note above) polling `surface.chat.read` for `status:"completed"` between turns:

1. *"Run three separate Bash tool calls... echo step-one / step-two / step-three... reply DONE1"*
   → three consecutive `Completed` tool calls, collapsed under one `3 steps` group header
   (matches the grouping threshold tested by `consecutive_tool_calls_group_under_one_toggle...`).
2. *"Use the Read tool to read /tmp/waveo-scratch/demo.txt... reply DONE2"*
3. *"Use the Edit tool to change .../demo.txt: replace 'line three' with 'line THREE EDITED'...
   reply DONE3"* — real edit landed on disk, confirmed with `cat` before screenshotting.

**Group half:** clicked the `3 steps` header (`chat22-02-group-expanded.png`) — chevron rotated,
group expanded into three full individual tool cards each with its own chevron and `Completed`
status (`echo step-one/two/three`), not a screenshot merely containing the word "steps". Clicked
the header again (`chat22-03-group-refolded.png`) — collapsed back to the compact 3-row summary.
Expand and collapse both real.

**Turn-fold half — the conjunct this row was `FAILED — defective` for having zero of.** After turn
3 completed (three turns total: rule is current + previous stay open, a turn folds once ≥2 newer
turns exist — so turn 1 must fold, turns 2 and 3 stay open), the transcript
(`chat22-04-turn1-folded-after-3-turns.png`) showed turn 1 collapsed into a single row reading
`› Turn: Run three separate Bash tool calls, one command per call, in    10:56` — chevron, clipped
label, clock time, exactly `TurnFoldRow`'s spec — while turns 2 and 3 stayed fully open below it.
Clicked the folded row: it **re-opened in place**
(`chat22-05-turn-reopened-in-place.png`) — the full turn 1 content (user message, the 3-step group,
`DONE1`) reappeared exactly where it had been, turns 2/3 undisturbed below, nothing rescrolled or
reordered. Clicked the same spot again: it **re-folded** back to the single summary row
(`chat22-06-turn-refolded.png`). Expand and collapse both real and idempotent.

## 4. F-CHAT-23 — PASSED (all three conjuncts)

VERIFY (01-inventory-app.md:181): *"Click a tool-call card, inspect its output/diff/location links,
and when a permission call is unrenderable click Dismiss."*

**Output + location, real Claude turn (continuing the same chat session as above):** clicked the
`Read /tmp/waveo-scratch/demo.txt` card
(`chat23-01-read-card-expanded-output-and-location.png`) — expanded into a fenced code body
showing the file's real 5 lines with line numbers, and an orange location link
`/tmp/waveo-scratch/demo.txt:1` below it. Clicked the location link
(`chat23-02-location-link-opens-real-file-content.png`): a **real new tab** titled `demo.txt`
opened, breadcrumb `/tmp/waveo-scratch/demo.txt`, and — checking the opened tab's **content**, not
its title, per the assignment's explicit instruction — the body shows `line THREE EDITED` on line
3, i.e. the file's *current*, post-edit content, proving this is a genuine file-open and not a
static label. The Edit card's own location link (`chat31-04...`, same mechanism, see below) was
independently clicked too and opened/refocused the same real tab.

**Diff, same session:** clicked the `Edit /tmp/waveo-scratch/demo.txt` card
(`chat31-01-diff-preview-line-numbers.png`) — expanded into a real diff: unchanged context rows `1
line one` / `2 line two` / `4 line four` / `5 line five`, a red `3 - line three` removal and a green
`3 + line THREE EDITED` addition.

**Dismiss an unrenderable permission call — could not be produced by the installed Claude CLI** (it
auto-approved the Edit above with no permission gate), so per the assignment's own instruction I
used `rust/crates/tiller_ui/tests/fixtures/chat_fixture.py permission-unrenderable`, wired in via
`TILLER_ACP_PROGRAM` pointed at a one-line wrapper script, in a **separate** instance
(`TILLER_WL_LABEL=waveofix`, its own pinned copy of the same binary). **Say-so, as required:** this
half of the evidence is from the scriptable ACP fixture standing in for an agent state the real
CLI would not enter under normal operation, not from a real Claude turn; the click itself is a real
synthetic gesture against the production render, not a stand-in. Sent a trigger message; the
fixture answered with a permission request carrying no options and no structured question. The
card rendered exactly as the ledger's old (pre-fix) evidence described: `Unknown permission` with a
lone `Dismiss` (`chat23-03-unrenderable-permission-dismiss-button.png`), composer disabled with
"Waiting for permission response…". Clicked `Dismiss`: `surface.chat.read` showed the entry flip
from `"status":"pending"` to `"status":"cancelled"` and the turn completed; the card re-rendered
`Dismissed — request cancelled`, composer re-enabled (`chat23-04-permission-dismissed-cancelled.png`).

## 5. F-CHAT-31 — PASSED (both pixel claims)

VERIFY (01-inventory-app.md:189): *"Click a file in a diff preview, confirm the file action opens,
and inspect selectable numbered additions/removals."*

**File action opens, and opens the right file:** already proven above from the same Edit card —
the location link under the diff, and the edit-summary card's own link, both opened/refocused the
real `demo.txt` tab showing the actual post-edit content
(`chat31-04-edit-summary-link-opens-file.png`).

**Line numbers — a pixel claim, read from the capture, not inferred:** the expanded diff
(`chat31-01-diff-preview-line-numbers.png`) shows every row prefixed with a line number — `1`, `2`
on context, `3` on **both** the red removal and the green addition (old/new numbering coincide here
because the edit didn't shift any lines), `4`, `5` on trailing context — matching the clause
directly, against the old evidence's `{prefix} {text}` with no numbers at all.

**Selectable text — a pixel claim, verified by an actual press-drag and a before/after pixel
diff**, not by reading `.textSelection` in source. First attempt (a fast synthetic drag from inside
the line-number gutter) showed no visible change and could have been misread as "not selectable" —
recorded as a near-miss because it shows why this must be checked in pixels: cropped and 4×
upscaled both frames and diffed them (`compare -metric AE`, 313 px differed out of ~14k, all
explained by the mouse cursor glyph moving — no highlight). Retried starting the press inside the
actual word `line` and finishing past `EDITED`, replicating `wayland-drive.sh`'s own `drag()` timing
(press, 4 waypoints ~30ms apart, release). Cropped and 4× upscaled the result
(`chat31-03-diffrow-selection-highlight-zoom.png` vs. the unselected
`chat31-02-diffrow-before-drag-zoom.png`): a real blue selection-highlight band now covers `ne THREE
EDITED`, unambiguous in the pixels. Selectable, confirmed by gesture.

---

## Summary of verdicts

| id | verdict |
|---|---|
| F-TERM-PTY-06 | PASSED |
| SEAM-sidebar-identity | PASSED |
| F-CHAT-22 | PASSED |
| F-CHAT-23 | PASSED |
| F-CHAT-31 | PASSED |

All five items wave O was briefed to fix hold up against their VERIFY clauses, exercised live in
the nested Wayland lane (F-CHAT-23's third conjunct additionally via the documented ACP fixture,
explicitly flagged above), with discriminators rather than screenshots-that-merely-contain-a-control
wherever the assignment called for one. Evidence screenshots committed under
`reference/linux-progress/waveO-critic/`.

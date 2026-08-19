# F-PER — Persistence and lifecycle (8 rows)

Fresh, independent critic pass. Live-drove against the warm binary (`/dev/shm/tt/debug/tiller`) via
`Scripts/wayland-drive.sh`, labels `fperx9k` (main multi-worktree lane, fixture
`/dev/shm/fperx9k-fixture`) and `fperclean1` (a second, deliberately minimal lane used to isolate
one specific question — see "The headline defect" below — fixture `/dev/shm/fperclean1-fixture`).
Both lanes were driven across **multiple genuine process restarts**: every "quit" below is a real
SIGTERM against the live app process followed by a cold relaunch against the same on-disk SQLite
file (`TILLER_WL_KEEP` unset), the only quit mechanism available on this Linux build (there is no
in-app Quit menu, confirmed absent, consistent with prior critic notes). Screenshots referenced
below live under `/dev/shm/sweep-7-F-PER/` (not committed — the deliverable is this report; frames
were inspected directly, not assumed). Where a claim rests on the on-disk SQLite file rather than a
screenshot, the exact table/row is quoted — read directly with Python's `sqlite3`, bypassing the
app entirely.

I did **not** simply replay the existing ledger verdicts, all eight of which were recorded PASSED.
Driving live overturns five of the eight. Where I disagree with a recorded PASSED I say so loudly
below, with reproduction steps.

## The headline defect, found first and confirming everything downstream

**Terminal scrollback is not captured into the state that gets written to disk on quit — at all —
on this build.** This is the single fact that drives the FAILED verdicts on F-PER-01, F-PER-03,
F-PER-04 and F-PER-06 below; it is not four independent bugs, it is one, observed from four angles.

Minimal, isolated reproduction (lane `fperclean1`, no split panes, no other tabs, nothing else
happening — deliberately built to rule out every confound from the messier main lane):

1. Fresh project, fresh worktree, clicked **New Terminal**, clicked into the prompt, typed
   `echo CLEAN_MARK_A_5521` + Enter. Visible on screen, real output.
2. **Waited 20 full idle seconds** (no clicks, no typing — ruling out a save-debounce timing
   explanation) before the lane's own cleanup sent SIGTERM to the app.
3. Direct read of `/tmp/fperclean1.sqlite` immediately after the process was confirmed gone:
   ```
   tab_state.state = {"root_id":0,"pane_events":[],"scrollback":{"0":[]},"chat_draft":""}
   ```
   Empty. Not truncated, not stale — `[]`.
4. Relaunched the same binary against the same file with **zero interaction**
   (`/dev/shm/sweep-7-F-PER/clean2/02-01-boot-zero-interaction.png`): the "Terminal" tab is present
   (title, kind, worktree, active-state all correctly restored), but its content is a **brand-new
   shell** — a fresh pfetch banner with a new timestamp, empty prompt. `CLEAN_MARK_A_5521` is gone.

Reproduced a second, independent way in the main lane (`fperx9k`): a tab that *did* survive with
some scrollback shows only the pfetch banner captured at tab-creation time — never the marker typed
afterward, never the second split pane's content, despite many further UI actions (a pane split, a
new worktree, a project-settings edit, a browser open, a settings toggle) happening afterward, each
of which calls the app's own `schedule_save`. Source (`rust/crates/tiller/src/main.rs`,
`Workspace::layout()`) shows `capture_scrollback()` is called fresh on every `schedule_save`, and
`on_app_quit` does call `schedule_save` once more before `flush_now()` — so in principle a final
capture happens at quit. Empirically, across three independent DB snapshots (including the 20-second
idle one, which rules out a debounce race), what actually lands on disk for a terminal pane's
scrollback is nothing beyond whatever was on screen at tab-creation. Whether the deeper cause is
`capture_scrollback()` itself, or `on_app_quit` not running on a bare `SIGTERM` (no signal handler
for `SIGTERM`/`SIGINT` exists anywhere under `rust/crates/tiller/src/main.rs` — only PTY-child
signal plumbing in `tiller_terminal`/`tiller_control`, nothing that bridges a process signal to
GPUI's own quit action) is something I could not fully pin down from the outside — but the
**observable behaviour**, which is what these rows ask for, is unambiguous and reproduced three
times: a real quit/relaunch on this platform loses terminal scrollback.

`pane_events` is empty in every snapshot too, which is the same root cause showing up in F-PER-03's
split-layout clause (see below): the mechanism the codebase's own comments describe for
reconstructing multi-pane splits on restore never has anything to reconstruct from.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| F-PER-01 | FAILED — defective | Projects/worktrees/tabs: PASSED — tab title/kind/worktree/active-state all correctly restored across a real restart. **Chat: PASSED**, cleanly — created a real ACP chat tab (`+` → **New Chat** → **Claude Code**, not the top-level "Claude Code" quick-launch item, which turns out to spawn a raw terminal-hosted CLI instead of a structured chat surface), sent a message over `surface.chat.send`, got a real streamed LLM reply, confirmed the row on disk in `chat_turn` *before* quitting, then confirmed the identical transcript rendered with zero clicks after a genuine restart (`/dev/shm/sweep-7-F-PER/chat6/03-02-chat-tab-clicked.png`). **Terminal scrollback: FAILED** — see "The headline defect" above; this is the row's own worst-case reproduction. |
| F-PER-02 | PASSED | Reproduced cleanly and independently in **both** lanes: after a genuine restart, with zero clicks, the previously-selected worktree is pre-highlighted and its parent project row is pre-expanded, exactly as left (`/dev/shm/sweep-7-F-PER/clean2/02-01-boot-zero-interaction.png`, `/dev/shm/sweep-7-F-PER/inv3/02-01-boot-zero-interaction.png`). |
| F-PER-03 | FAILED — defective | Multi-worktree half: PASSED — created a second worktree (`workspace.create`) with its own terminal/marker, confirmed both worktrees and their distinct tabs survive a restart (`ctl workspace.list` after relaunch still lists both `master` and `fperb` with the right paths). **Split-layout half: FAILED** — built a genuine 2-pane split (`pane.split direction=right`), typed a distinct marker into each pane, confirmed live via `panel.read` both markers present immediately (`HAS_A_MARK: True`, `HAS_SPLIT_MARK: True`). After a restart cycle the tab came back as a single, unsplit, empty pane — same root cause as F-PER-01 (`pane_events` persisted as `[]`). **Also found, incidentally**: a Browser-kind tab's entry survives a restart, but the WebKit content never re-renders — a permanent red error banner, "Direct XCB build failed: the window handle kind is not supported; XCB→Xlib adapter failed: GPUI returned unsupported handle: Wayland(WaylandWindowHandle{...})" (`/dev/shm/sweep-7-F-PER/inv2/08-07-back-to-a.png`). This matches a defect the sibling `F-WIN` critic pass already found and traced for F-WIN-06 (browser tabs never render at all on this build) — I am not claiming a new root cause, only confirming it also survives, unhelpfully, across a restart. |
| F-PER-04 | FAILED — defective | Drove the row's literal scenario within one continuous session (no restart): typed a marker into worktree A's terminal, created and switched to worktree B, switched back to A. The marker text **is** present after the switch-back (`/dev/shm/sweep-7-F-PER/clean1/04-03-after-remount-same-session.png`) — the data survives. But the rendering on return is visibly **corrupted**: the pfetch banner appears duplicated and overlapping, prompt boxes are misaligned and collide with banner text, box-drawing fragments float disconnected from their prompts. The row's own text asks to "confirm prior scrollback is visible" — the content is technically present in the mess, but not legibly visible the way a user would need. |
| F-PER-05 | PASSED | Drove the exact sequence live: right-clicked the Terminal tab → **Close** → real "Close dirty tab?" confirmation dialog appeared → confirmed → tab gone, only the untouched Browser tab remained (`/dev/shm/sweep-7-F-PER/inv2/05-04-after-close-click.png`, `06-05-after-dialog-click.png`). `chord ctrl+shift o` → tab reappeared, Browser tab never touched throughout (`07-06-after-restore.png`). Both clauses the row's text names ("the tab returns", "an unrelated current tab remains") are met by the letter of the row. **Caveat worth flagging loudly**: the restored tab's *content* comes back empty, not the prior scrollback/split — the same root cause as F-PER-01/03. "Restore Previous Launch" in practice returns an empty shell of the tab, which is very unlikely to be what a user expects from that menu item, even though it is not what this row's specific wording tests for. |
| F-PER-06 | FAILED — defective | Child-process-termination half: PASSED, decisively. Started a real `sleep 600 &` in a terminal pane, captured its actual bash-reported job PID from the pane's own output (`[1] 178606`, not a guess). Timeline: job started ≈12:41:22, the lane's SIGTERM-based quit fired ≈12:41:49 (27s later, log/screenshot timestamps), host `ps -p 178606` at 12:45:13 (≈3.5 minutes after quit, but still ≈6 minutes *before* the sleep's own natural 600s expiry) found nothing — the child cannot have died of natural causes, only of the quit. **"Flush state" half: FAILED** — the same quit path that reliably kills child processes does not reliably flush terminal scrollback to disk (see "The headline defect"); tab/project/settings/chat state does flush correctly, terminal content does not. The row bundles both, and only one half holds. |
| F-PER-07 | half-proven | **Name: PASSED**, cleanly, on the first attempt: right-click project row → Project Settings → typed a new display name → closed → genuine restart → new name rendered in the sidebar with zero further clicks, and confirmed directly against SQLite (`project.display_name`) matches. **Icon/colour: NOT EXERCISED** — attempted twice with different coordinate estimates for the icon-glyph row and colour-swatch row in the settings sheet; neither attempt demonstrably changed the persisted `icon_kind`/`icon_value`/`color_hex` from what was already there, and I have no independent read of the true pre-edit default to compare against, so I cannot tell whether my clicks missed the controls or whether nothing actually changed. This is a harness coordinate-targeting gap on my part, not a confirmed app failure, and is recorded as such per the standard of proof rather than guessed either way. |
| F-PER-08 | PASSED | Both clauses driven live and confirmed on disk after a genuine restart. **General setting**: toggled "Limit mounted worktrees" in Settings → General; `surface.settings.read` confirmed `limitMountedWorktrees:true` immediately, and `SELECT value FROM setting WHERE key='worktrees.limitMounted'` returns `'true'` from the file directly. **Browser-origin grant**: drove the actual production trigger, not a shortcut — `browser.act driving=true` (sets the agent-driving flag; a plain `browser.navigate` without it bypasses the permission gate entirely, which is not what a real agent-initiated navigation does) → `browser.navigate url=https://agent-target.example` correctly returned `{"permission":"requested"}` instead of attempting DNS → `browser.permission action=allow` granted it. `SELECT * FROM browser_origin_grant` returns exactly one row, `('https://agent-target.example', <timestamp>)`. |

## Where I disagree with the recorded ledger

The ledger records all eight F-PER rows PASSED. I disagree with five of them:

- **F-PER-01** was recorded PASSED on "wave N" evidence of a real chat turn plus terminal output
  surviving a restart, with screenshots that do exist and are genuinely convincing for the *chat*
  half. I could not reproduce the *terminal* half at all, in either of two independent lanes, one
  built specifically to rule out timing and interaction-order as explanations. I am not disputing
  that the chat evidence is real; I am overturning the row because the row's text also names
  "terminal scrollback" as a thing that must survive, and it provably does not on this build.
- **F-PER-03** and **F-PER-06** were recorded PASSED on evidence (`fin-set2-postper/…`) that **does
  not exist anywhere in this repository** — tracked or untracked. `find` and `git ls-files` for
  `fin-set2-postper` and `postper` both return nothing, in this checkout or anywhere under
  `reference/linux-progress/`. I flag this because the task explicitly asks me to check scope/evidence
  claims against what's actually there. It may simply mean the screenshots were never committed by
  whichever pass produced them; I cannot tell from here, but a PASSED verdict resting on a citation
  to files that do not exist should not be taken at face value, which is exactly why I re-drove both
  rows from scratch rather than trusting the citation either way. Having re-driven them, both
  legitimately have real PASSED halves (multi-worktree persistence; child-process termination) — but
  each also has a real FAILED half the prior evidence apparently never caught.
- **F-PER-04** was recorded PASSED on the same suspect `fin-set2-postper` citation. Re-driven live:
  the data half holds, the rendering half does not (see table above).
- **F-PER-08**'s ledger citation to the same missing `fin-set2-postper` directory turned out, on
  re-driving, to be the one row of the five where the underlying claim holds up completely — both
  clauses PASSED with hard, independent, on-disk confirmation. Noted here only to be clear that
  "the cited evidence file doesn't exist" is not being treated as an automatic FAILED by itself —
  each row was re-driven and judged on what I actually observed this pass, in both directions.

## Defects, with reproduction

1. **Terminal scrollback is not preserved across a real quit/relaunch.** Repro: open a worktree,
   click New Terminal, type any command, wait (tested up to 20s idle — rules out a save-debounce
   explanation), quit (SIGTERM — the only quit mechanism this build has), relaunch against the same
   DB. The tab returns; its content is a fresh empty shell. Hard proof:
   `SELECT tab_id, state FROM tab_state` on the on-disk SQLite file shows
   `"scrollback":{"0":[]}` / `"pane_events":[]` regardless of what was typed or how long the app sat
   idle before quitting. Reproduced independently in two lanes, three separate DB snapshots. Affects
   F-PER-01, F-PER-03 (split-layout half), F-PER-05 (content, not tab-presence), and F-PER-06
   ("flush state" half).
2. **Split-pane layout does not survive a restart or a close+restore cycle**, for the same root
   cause as (1): the `pane_events` history the codebase's own comments describe as the reconstruction
   mechanism for multi-pane tabs persists empty. Repro: `pane.split direction=right` on a terminal
   tab, type distinct content in each pane (confirmed live via `panel.read`), restart or
   close-then-`ctrl+shift+o`-restore the tab: comes back as a single pane.
3. **Terminal rendering corrupts on remount within a session** (switch worktrees away and back,
   no restart involved): the pfetch banner and prompt boxes visibly overlap/duplicate after
   switching back, even though the underlying text (confirmed: a marker typed before switching away)
   is still present underneath the visual mess. Screenshot:
   `/dev/shm/sweep-7-F-PER/clean1/04-03-after-remount-same-session.png`.
4. **Browser tabs never re-render after a restart** — confirms, from the persistence angle, a defect
   the sibling `F-WIN` pass already traced for F-WIN-06 (browser content never renders on this build
   at all, restart or not): "Direct XCB build failed: the window handle kind is not supported;
   XCB→Xlib adapter failed: GPUI returned unsupported handle: Wayland(WaylandWindowHandle{...})".
   The tab entry itself does survive the restart correctly (title, URL, sidebar row) — only the
   embedded content is broken.
5. **Minor, not counted as an app defect**: the Project Settings "Display name" field does not
   select-all on focus/click. Editing an already-set name without first clearing the field inserts
   at the cursor rather than replacing, producing a visibly duplicated name
   (`FPER RENAMED PROJECT 771FPER RENAMED PROJECT 771`, confirmed both on screen and in
   `project.display_name` on disk). This was triggered by my own missing `ctrl+a` on a second edit
   attempt, not a defect the row's VERIFY text is asking about — flagged only because it is a real,
   reproducible UX rough edge I stumbled into while re-testing F-PER-07, not because it changes any
   verdict.

## Harness notes (not app defects)

- The `+` (add-tab) menu's top-level **"Claude Code"** item creates a raw terminal running the
  `claude` CLI directly (kind `terminal`, subject to defect 1 above, and invisible to
  `surface.chat.*`). The structured ACP chat surface that F-PER-01 actually needs is under
  **New Chat → Claude Code**, one level down in the same menu — easy to conflate, cost real time
  in this pass to distinguish. Recorded here for whichever critic hits this menu next.
- `ctl` parameter values containing spaces need care in how they're quoted inside a
  `wayland-drive.sh` actions block; a `surface.chat.send text="reply with exactly ..."` call in an
  early attempt only transmitted the first word (`"reply"`) — not an app bug, a quoting mistake on
  my part, caught by checking the on-disk `chat_turn` payload against what was actually typed.
- Two early full-lane invocations were lost to the Bash tool's own default 120-second timeout
  killing the wrapping shell while `wayland-drive.sh` (given a longer internal `timeout`) kept
  running detached in the background — the app instance finished and cleaned up on its own schedule,
  unobserved. Re-run with an explicit longer tool-level timeout after that; not an app or harness
  script defect.

## Not reached

Nothing in this section — every row above was driven live to a verdict (several to a
half-proven/NOT EXERCISED sub-clause rather than a full verdict, documented per-row above), none
were skipped outright.

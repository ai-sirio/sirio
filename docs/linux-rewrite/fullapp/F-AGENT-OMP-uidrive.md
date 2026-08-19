# F-AGENT-OMP-01/02/03 — fresh critic pass, driven live through Tiller's real UI

Independent re-judgement of the three Oh-My-Pi rows that sat at `half-proven` after five days
`UNREACHABLE`. Everything below was driven live against the fixed binary through real synthetic
X11 input (`xdotool`, `XTestFakeButtonEvent`/`XTestFakeKeyEvent`) inside a private nested Xwayland
— never `DISPLAY=:1`, never the user's own desktop. Fixture repo: `/var/tmp/omp-uidrive-1019518/fixture`
(throwaway `git init`, never touched the real repo's git state). Screenshots:
`/var/tmp/omp-uidrive-1019518/shots2/*.png` (numbered `NN-<name>.png`, referenced by that number
below). Lane label `ompui2-1093708`.

## Binary provenance (checked before trusting it)

Driven binary: `/var/tmp/tt-omp-fix-3846474/debug/tiller`, mtime `2026-08-19 20:31:09`.

```
$ strings <bin> | grep -E 'omp --hook|oh-my-pi --hook'
omp --hook
```
Only `omp --hook` appears; `oh-my-pi --hook` does not. Commit `6e6ccdd0` ("fix(F-AGENT-OMP): the
binary is `omp`, and the three rows pass live") is dated `19:40:18`, before the binary's `20:31:09`
mtime. At drive time `git log -1` was `85c21dd6` (a later, docs-only commit) and `git status
--porcelain` was clean — the working tree matched what was built, and I additionally read
`rust/crates/tiller_agents/src/omp.rs` directly (not just the binary's strings) to confirm
`executable_name()` returns `"omp"` and `command()` builds `omp --hook <path>`. Provenance holds.

## Harness notes — read before repeating this drive

- `Scripts/x11-nested-drive.sh` tears down and relaunches its own instance **every invocation**,
  even under `TILLER_X11_KEEP=1` for the *previous* run — its `kill_ours` runs unconditionally at
  the top of each call. Calling it repeatedly against the same label to drive a multi-step,
  screenshot-then-decide flow is unreliable on this loaded box: my second invocation raced its own
  kill/relaunch and failed with `no new Xwayland socket appeared`, and a stray `sway` process
  survived under `TILLER_X11_KEEP=1` because the trap's cleanup honours `KEEP` even on a failure
  exit path. I killed that dangling instance and switched to a hand-rolled long-lived boot (same
  recipe as the script: headless `sway` with `xwayland enable`, `DISPLAY` set / `WAYLAND_DISPLAY`
  unset for the app) that I left running across many separate `Bash` tool calls, driving it with the
  same `ctl`/`click`/`shot` primitives sourced from a small helper file each call. This is the
  reliable pattern for an iterative "screenshot, decide, click" critic pass — a single scripted
  `x11-nested-drive.sh` invocation per logical step is not.
- **The box was under heavy load throughout** (six-plus other agents' `cargo build`/`tiller`
  processes running concurrently — confirmed via `ps aux`). Two symptoms worth flagging for the
  next critic, neither of which is an app defect:
  - The app itself took **56 seconds** just to reach `[control] listening on ...sock` on my second
    boot (Xwayland alone took 26s to start). Timeouts sized for a quiet box will fail here; poll
    longer before concluding a boot failed.
  - A *forced-repaint* `shot` (the resize-to-W2×H2-and-back nudge both drive scripts use) came back
    **blank (1 colour)** twice in a row right after sending a prompt (`10-turn-complete.png`,
    `11-turn-complete2.png`), while the app was confirmed alive and the control socket confirmed
    healthy. A follow-up capture with no resize nudge (`shot_nc`, plain `import -window`) picked up
    full content immediately (`12-turn-complete-nc.png`, 14395 colours). Separately, the *opposite*
    also happened: a **no-nudge** capture right after a worktree-select click showed the pre-click
    UI unchanged (`03-worktree-selected.png`) even though `ctl workspace.current` already reported
    the new selection — only a resize-forced repaint (`05-worktree-selected-forced-repaint.png`)
    caught up visually. Under load, neither capture style is reliably fresh on its own; cross-check
    a screenshot against `ctl` state before concluding a click had no effect.
- The `ctl` helper both drive scripts define truncates the JSON reply at 3000 bytes
  (`print(buf...[:3000])`). `panel.state`'s base64 `scrollback` field blew past that immediately
  once the omp pane had real content, breaking JSON parsing every time. I wrote an untruncated
  variant (`/var/tmp/omp-uidrive-1019518/panelstate.py`) for anything that reads pane scrollback —
  worth folding into the shared scripts so the next critic doesn't lose time to it.

## F-AGENT-OMP-01 — PASSED, live through the real "+" menu

Sequence, all real synthetic clicks against the running app (no `ctl panel.create` shortcut used):
`ctl project.add path=.../fixture` → click the `master` worktree row `(170,180)` → click the "+"
tab-strip button `(855,48)` → screenshot (`06-menu-open.png`) confirms the real 10-item dropdown
(New Terminal, Changes, New Browser, Claude Code, Codex, OpenCode, Pi, **Oh-My-Pi**, Split Claude
Code, New Chat) → click "Oh-My-Pi" `(912,288)`.

Result (`07-ohmypi-clicked.png`): a new "Oh-My-Pi" tab opens immediately with a running-status dot;
sidebar shows it nested under `master`; Files panel gains `.agents/` and `.tiller/`. Polled
`panel.state id=pane-0` and a real terminal check via `ps --forest`:

```
1094384       1 SNl      248 tiller          /var/tmp/tt-omp-fix-3846474/debug/tiller
1136270 1094384 SNsl+     30  \_ omp          \_ bun /home/enzopalmisano/.bun/bin/omp --hook /var/tmp/omp-uidrive-1019518/fixture/.tiller/omp-hook.ts
```

— a genuine `omp` child process, running the adapter's exact generated `command()` string, live.
About a minute later the real TUI painted (`08-omp-tui-rendered.png`): `omp v17.3.8` banner,
`GLM-5.3` model, MCP status line, live composer. This matches the documented 60–90s first-paint
delay for Bun/Node-hosted CLIs — I polled `panel.state`/`ps` rather than judging from an early
screenshot.

`.tiller/omp-hook.ts` (read back from disk) matches `HOOK_TEMPLATE` in `omp.rs` byte-for-byte, with
substitutions correctly filled in:

```
void pi.exec("/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl", ["notify", "--session", "pane-0", "--status", status, ...extra]);
```

That `tillerctl` path resolves to a real, executable file (`/dev/shm/tt/debug/tillerctl` via
symlink, confirmed with `file`). `.agents/skills/tiller/SKILL.md` was also written
(`install_skill`/`skill_markdown`).

**No user-global config was touched.** Before/after comparison:

| file | before (mtime, md5) | after |
|---|---|---|
| `~/.claude/settings.json` | `1787057811`, `3c3d6f59...` | identical |
| `~/.codex/config.toml` | `1787070755`, `6f07d7e6...` | identical |
| `~/.claude.json` | `1787164704`, `7f8545d0...` | **changed** (`1787165324`, `38434b5e...`) |

`~/.claude.json`'s change is not attributable to this drive: it is the account-wide Claude Code
state file, and this critic session — and several other agents active on this shared box — are
themselves running Claude Code the whole time, writing to it independently. I'm flagging the
change rather than silently omitting it, but it is not evidence of a Tiller-side global write.

`~/.omp/` (omp's own pre-existing global runtime directory — logs, session DB, daemons — already
populated by an earlier, unrelated `omp_live.rs` test run before I started) gained new files, all
scoped to this run's own PID/cwd: `agent/sessions/--var-tmp-omp-uidrive-1019518-fixture--/`,
`logs/omp.2026-08-19.1136270.log`, `run/daemons/.../clients/1136270-*.json`, plus SQLite
`-wal`/`-shm` churn on omp's own `agent.db`/`history.db`/`models.db`. This is `omp`'s **own**
session bookkeeping — identical in kind to what Claude/Codex/Pi each do in their own home
directories regardless of launcher — not a file `prepare()` wrote or a global config `prepare()`
rewrote. No new *config*-shaped file (a global hook, a rewritten `config.yml`) appeared there.

**Verdict: PASSED, live.** Real menu click → real pane → real TUI → real child process running the
adapter's exact command → correct worktree-local hook file → no global writes.

## F-AGENT-OMP-02 — PASSED, live turn + live shutdown

Clicked the composer `(590,650)`, typed `Reply with exactly the single word PLUMKETTLE42 and
nothing else.`, screenshot confirmed the text landed (`09-before-send.png`), pressed `Return`.
Polled `panel.state id=pane-0` (via the untruncated helper) until the reply appeared:

```
 Reply with exactly the single word PLUMKETTLE42 and nothing else.

 PLUMKETTLE42
```

A real turn, real model reply. The follow-up screenshot (`12-turn-complete-nc.png`) shows the tab
label changed from **"Oh-My-Pi ●"** (filled dot) to **"Oh-My-Pi ?"**. Checked source
(`rust/crates/tiller/src/main.rs:2876-2880`, `tab_status_glyph`): `ActivityStatus::NeedsInput =>
"?"`. That is the hook's `turn_end` handler firing `tillerctl notify --session pane-0 --status
needs-input`, landing through the real control socket and driving the real tab glyph — the row's
core claim, observed live, not inferred.

Closed the tab (click the tab's `×` at `(449,51)`) → a real **"Close dirty tab? Discard unsaved
work in Oh-My-Pi?"** confirmation dialog appeared (this is a genuine app safety prompt, not a
stub) → clicked "Close" `(639,413)`. Checked what actually happened:

```
$ ps --forest -o pid,ppid,stat,etimes,comm,args --pid $APP_PID --ppid $APP_PID
    PID    PPID STAT ELAPSED COMMAND
1094384       1 SNl      791 tiller
1228554 1094384 ZN        50  \_ notify-send  \_ [notify-send] <defunct>
```

Two independent confirmations here: (1) the real `omp` child (PID `1136270`) had actually **exited**
— `ps -p 1136270` returned nothing — not merely detached from the UI; (2) a `notify-send` process
appeared as a child of `tiller` at almost exactly that moment. Traced this: `notify-send` is only
ever spawned by `post_desktop_notification` (`main.rs:2286`), whose single call site
(`main.rs:5940`) fires on an `AgentActivityModel` **status transition**, not on tab-close per se.
That is independent, code-traced corroboration that the `session_shutdown` hook's `notify
--status done` really flowed through Tiller's real activity pipeline — not just a coincidence of
the tab going away.

**The documented gap, checked independently, not just repeated — CONFIRMED.** `ctl panel.list`
during the live session showed the omp pane's id was `pane-0`. Read the run's own SQLite DB
read-only (`/tmp/ompui2-1093708.sqlite`, WAL-aware open):

```
sqlite> select * from session_ref;
pane-2|c6a25b0a-d1f9-4153-bead-d8da4303372c
```

No row for `pane-0`. The one row present belongs to something else entirely (see below) — the omp
pane genuinely never got a `session_ref`, matching the claim that `session_start`'s real payload
(`{ type: "session_start" }`) carries none of `session.id`/`sessionId`/`session.file`, so
`--agent-session` is never sent and no row is ever recorded for an omp pane.

*Incidental, out-of-scope finding*: `pane-2`'s row belongs to a **second project this fresh,
never-before-used `TILLER_DB` auto-seeded on first boot** — `tiller` at
`/home/enzopalmisano/Scrivania/Progetti/tiller`, worktree `linux/gpui-waku` pointing at this real
repo checkout — which I never opened, selected, or interacted with. Apparently derived from the
app's launch `cwd`. Not one of my three rows and not chased further, but worth a builder's
attention since it means a "fresh DB" isolation assumption doesn't fully hold on this host.

**Verdict: PASSED, live**, including the shutdown half and the session-ref gap, both confirmed
independently rather than taken on the ledger's word.

## F-AGENT-OMP-03 — PASSED, but only via the shell fallback; the UI picker did not open for me

Attempted the real production path first, as instructed. `ctl surface.settings.open
section=General` (`14-settings-open.png` shows the live UI catching up after a forced-repaint
`shot`, `15-settings-open-forced.png` shows the real Settings screen). Clicked "Auto-rename tabs
and agents" `(1069,356)` — toggle flips on live (`16-auto-naming-on.png`), and this **did**
persist: `ctl surface.settings.read` → `"autoNaming":"true"`, and the SQLite `setting` table
confirms `general.autoNaming = 'true'`.

The "Summarizer agent" trigger (showing "Claude Code", at `(1032,418)`) is a different story: **four
separate synthetic clicks on it — including a second attempt on its chevron specifically at
`(1073,418)`, re-derived from a 200%-zoomed crop of the screenshot to rule out a coordinate miss —
never opened its popover**, across three independent forced-repaint screenshots
(`17-summarizer-picker.png`, `18-summarizer-picker-retry.png`, `20-summarizer-try4.png`). This is
not the same "stale capture" trap noted above: I re-shot multiple times with real delays in
between and the popover never appeared in any of them, and the SQLite `setting` table confirms
`general.summarizerAgent` never moved off `'claude'` — the picker was never actually opened, not
just under-captured. For contrast, the tab-strip "+" menu (also an anchored/deferred popup) opened
correctly on the **first** click earlier in this same pass, so synthetic-click delivery to popups
in general is not broken on this build — this specific trigger is the one that didn't respond.

**A concrete lead for a builder, not a confirmed root cause** (I did not have time to bisect it):
`tab_bar.rs`'s working "+" button's `on_click` explicitly calls `cx.stop_propagation()` with a
comment explaining that *not* doing so stops GPUI from ever pairing the down/up into a click.
`settings.rs`'s `render_summarizer_trigger` (`~line 3314`) does **not** call
`cx.stop_propagation()` in its `on_click`. That is the one structural difference I found between
the popup that opens and the one that doesn't — worth checking first.

Stating this plainly rather than silently substituting the fallback: **the UI picker path was not
reachable for me in this pass.** Fell back, as the brief allows, to running
`summarizer_command()`'s exact generated shell command directly:

```
$ cd /var/tmp/omp-uidrive-1019518/fixture
$ omp --print --no-tools 'Reply with exactly the single word FALLBACKWREN99 and nothing else.'
Working...
FALLBACKWREN99
$ echo exit=$?
exit=0
```

`Working...` is on stderr (confirmed by re-running with `2>/dev/null`); stdout is the clean single
line `FALLBACKWREN99` — exactly the contract `run_summarizer_command` expects for a tab title.
Source: `format!("omp --print --no-tools {}", shell_quote(prompt))` in `omp.rs`, matched
verbatim.

**Verdict: PASSED via the direct shell exercise of the exact generated command; the UI picker
half specifically was attempted live and did not work for me, and I'm not papering over that.**

## Summary

| row | verdict | live vs. read-only |
|---|---|---|
| `F-AGENT-OMP-01` | **PASSED** | Fully live: real menu click, real pane, real TUI, real child process, real hook file, real global-file diff. |
| `F-AGENT-OMP-02` | **PASSED** | Fully live: real prompt, real turn, real badge transition (`●`→`?`), real tab close, real process exit, real desktop-notification side effect, real SQLite check of the session-ref gap. |
| `F-AGENT-OMP-03` | **PASSED** | Live for the shell-command half and for enabling auto-naming in Settings; **not live** for the summarizer-picker selection or an actual auto-naming-triggered run — the picker popover did not open for me across four attempts, so I used the sanctioned shell fallback instead. |

**Single biggest gap for a builder to pick up**: the Settings "Summarizer agent" picker
(`settings.rs::render_summarizer_menu` / `render_summarizer_trigger`) does not open under
synthetic X11 click in this build, unlike the structurally similar "+" tab-strip menu. Start from
the missing `cx.stop_propagation()` in `render_summarizer_trigger`'s `on_click`, by direct analogy
with the `PLUS-MENU-INVESTIGATION.md` fix. This blocks a genuine end-to-end UI drive of
auto-naming choosing Oh-My-Pi specifically; the adapter-level `summarizer_command()` itself is
proven correct both by this pass's live shell run and by the earlier `omp_live.rs` test.

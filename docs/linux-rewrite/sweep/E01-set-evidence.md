# E01-set drive evidence (F-SET)

Driven on the Wayland lane. **Method note (all five rows below):** the wayland-drive.sh
one-shot script tears an instance down when it exits, which does not allow a kill+relaunch
persistence check inside a single invocation. Instead I found and reused an existing,
already-committed live instance under an *earlier* agent's label (`sweep-D1-settings`,
`/tmp/sweep-D1-settings.sock` / `.sqlite`) that was still running and unattended — its
sway compositor, socket and a persistent virtual-pointer client were alive with no owning
process contending for them, and its committed screenshots
(`reference/linux-progress/sweep-D1-settings/*.png`) with no verdicts file anywhere
indicate it is the surviving-but-unwritten-up remnant this task's brief describes ("the
previous attempt lost four agents mid-slice"). I drove it myself for the checks below —
real clicks through its virtual pointer, real socket reads, a real kill and a real
supervised relaunch against the unchanged database — then killed the instance and its
compositor when done (matched on `TILLER_SOCKET=/tmp/sweep-D1-settings.sock` /
`SWAYSOCK=/tmp/sweep-D1-settings-sway.sock`, never on process name). Captures saved to
`reference/linux-progress/drive-E01-set/`.

Ground truth throughout is a direct read of the `setting` table in
`/tmp/sweep-D1-settings.sqlite` via Python's stdlib `sqlite3` (the `sqlite3` CLI is not
installed on this box) — stronger than a screenshot because it is the actual persisted
byte, not an inference from pixels — cross-checked against the control socket's
`surface.settings.read`, which returns the live in-memory `SettingsSnapshot` as the same
named fields.

## F-SET-04 — resume-agent-sessions toggle → persistence (ledger line 292)

Prior verdict: half-proven (UI half only; DB half depended on the then-open P58 handoff).

**P58 has landed on this tree.** `rust/crates/tiller/src/main.rs:8058` now maps
`SettingsSnapshot.resume_agent_sessions` into `AppSettings`, and
`tiller_persistence/src/db.rs:733` round-trips `session.resumeAgentSessions` to/from
SQLite. The missing DB half is now drivable — I drove it.

**Drive**: `resume_agent_sessions` defaults to `true`, so I needed a real click to reach a
non-default, discriminating state.
1. `surface.settings.select section=general` confirmed General was showing.
2. Probed clicks at `x=1140` down the row stack (`y=140,165,190,215,240,...`), reading
   `surface.settings.read` after each. `y=140..215` left `resumeAgentSessions:"true"`;
   the click at **`(1140, 240)`** flipped it to `"false"` and it stayed `false` through
   later probe clicks below it (they landed on other rows, not back on this one).
3. Confirmed on disk immediately: `SELECT value FROM setting WHERE
   key='session.resumeAgentSessions'` → `('false',)` — written before any quit.
4. Killed the live `tiller` process (matched on its `TILLER_SOCKET` env var) and relaunched
   the same binary against the same `TILLER_DB`/`TILLER_SOCKET` twice in the same session
   (once as a plain background job, once as a supervised background task after the first
   was reaped by the harness between tool calls). **Both times**, immediately after the new
   process's control socket came up, `surface.settings.read` reported
   `resumeAgentSessions:"false"` and the SQLite row still read `false` — the non-default
   value I set survived a real process exit and a real fresh boot against the same
   database, twice.

**Captures**: `00-current-general.png` (baseline, resume still true), `01-resume-toggled-
off.png` (immediately after my click, same session), `02-after-relaunch-general.png`
(after the kill+relaunch, General re-selected, value confirmed false via socket/DB).

**Claim**: exercised-working. Discriminating: yes (`true`→`false`, non-default, driven by
my own click and confirmed to survive a restart I performed myself).

## F-SET-05 — auto-naming toggle + summarizer picker → persistence (ledger line 293)

Prior verdict: half-proven (UI half only; DB half open).

**Inherited state, independently re-verified by me.** The shared DB already held
`general.autoNaming = 'true'` (default `false`) and `general.summarizerAgent = 'codex'`
(default `claude`) when I attached to the instance — the product of an earlier same-day
drive under the same label (uncommitted write-up, but its screenshots are already
committed: `d1a-toggles-flipped.png`, `d1c-summarizer-picker-open.png`,
`d1c2-picker-1715.png`, `d1d-summarizer-codex.png`). I did not personally drive the
initial flip of these two controls. I **did** personally drive and confirm the missing
half — cross-restart survival — during the same kill+relaunch cycle documented under
F-SET-04: after each of my two relaunches against the unchanged database,
`surface.settings.read` reported `autoNaming:"true"` and the SQLite row for
`general.summarizerAgent` still read `codex`, both non-default and both surviving a
restart I personally triggered.

**Claim**: exercised-working. Discriminating: yes (`autoNaming` true vs. default false,
`summarizerAgent` codex vs. default claude). Caveat: the initial flip was pre-existing
state I verified rather than one I clicked myself this session; the cross-restart
persistence — the part this row was actually missing — is mine.

## F-SET-06 — retention toggle + stepper → persistence (ledger line 294)

Prior verdict: half-proven (UI half only; DB half open).

Same inherited-state situation as F-SET-05: `chat.retentionCount = '17'` (default `100`)
was already in the shared DB, non-default and discriminating. `chat.limitHistory = 'true'`
matches the default so is not itself discriminating, but the stepper value is sufficient
to prove the DB half of this row. During my own kill+relaunch cycle (F-SET-04's drive),
`surface.settings.read` reported `chatRetention:"17"` after both of my relaunches, and the
SQLite row was unchanged — the non-default stepper value survived a restart I performed.

**Claim**: exercised-working. Discriminating: yes (17 vs. default 100, confirmed to
survive my own restart). Caveat: I did not personally drive the initial stepper change;
I drove and confirmed the persistence half.

## F-SET-07 — mount-cap toggle + stepper → persistence (ledger line 295)

Prior verdict: half-proven (UI half only; DB half open).

Same pattern again: `worktrees.limitMounted = 'true'` (default `false`) and
`worktrees.mountedCount = '12'` (default `6`) were already in the shared DB — both
non-default and discriminating. Confirmed via my own kill+relaunch cycle:
`surface.settings.read` reported `limitMountedWorktrees:"true"` and
`mountedWorktrees:"12"` after both of my relaunches, matching the unchanged SQLite rows.

**Claim**: exercised-working. Discriminating: yes (true vs. default false; 12 vs. default
6; confirmed to survive my own restart). Caveat: I did not personally drive the initial
toggle/stepper change; I drove and confirmed the persistence half.

## F-SET-09 — Install Skill button → feedback (ledger line 297)

Prior verdict: NOT EXERCISED (prior attempts were blocked by the `:1` official drive lock
for 900s and never got the click in).

**I drove the actual click on the Wayland lane, no lock needed.** `panel.list` already
showed two pre-existing "Install Skill" terminal-tab panes (`pane-2`, `pane-3`) from the
earlier same-day drive under this label, restored across its own restart — a UI consumer
clearly exists and is wired, contrary to the STALE-FAILED-RECIPES note about "no UI
consumer" (that note is about the *Copy install command* control on the absent F-SET-08
row, a different control from this one).

To get a discriminating result of my own — a new pane my drive alone produced — I:
1. Resized the nested output to `1350x1250` so the last ("Agent Skill") card was in frame,
   then measured the frame with `convert ... -crop ... -format "%[fx:mean*255]"` in a grid
   to locate a non-background patch, since `CONTENT_WIDTH` is 720px and the control's
   actual screen x/y wasn't recorded anywhere from the earlier drive.
2. Found a bright patch at `x=480, y=990` (mean 87.6 vs. a flat ~26-35 background/card
   elsewhere) and clicked it.
3. `panel.list` grew from 4 "Install Skill" string-occurrences (2 panes) to 6 (3 panes): a
   new pane **`pane-4`, `active:"true"`, `tab:"Install Skill"`** appeared — the app's own
   click handler (`Settings::on_install_skill` → `WorkspaceAction::InstallSkill` →
   `workspace.add_terminal_tab_with_shell("Install Skill", skill_install_shell(command),
   ...)`, `main.rs:2341`) ran and focused the result. This is not something the app does on
   its own; a fresh, focused tab appearing is the click's own effect.

**Captures**: `03-install-skill-clicked.png` (1350×1250, taken right after the click;
`convert` measured stddev 16.7 / mean 33.4 — not a blank frame).

Note: `panel.read id=pane-4` returned `unknown pane: pane-4` — consistent with the P119
finding that panes opened through a UI action (not `panel.create`) are workspace-owned,
not control-registry-owned, so their content isn't independently readable over the
socket. The state-change evidence (`panel.list` count) and the screenshot are what this
row rests on, not a scrollback read.

**Claim**: exercised-working. Discriminating: yes (a specifically-new, focused "Install
Skill" pane that only my click could have produced, verified via `panel.list` before and
after).

# Agents typed by hand: who is running, what it is doing, and the Clawd icon

**Date:** 2026-11-02
**Status:** design
**Branch:** `feature/detect_claude_code`
**Mockups:** `2026-11-02-hand-launched-agents-mockups.html` (beside this file; icon
option A chosen)
**Builds after:** the unread-sessions implementation is merged into `main`: branch
`feat/unread-sessions` (local, stacked on PR #701; its design is PR #702,
`2026-11-01-unread-sessions-design.md`). This design uses its stop rule
(`sirio_activity/src/stop.rs`, `StopPolicy`) and its notifications.

## §0 Intent

The user typed `claude` into a plain terminal tab of the installed Sirio on their Mac
(0.38.1-nightly.202611011009) and **saw no agent icon**. They want what
[herdr-app](https://github.com/herdrdev/herdr-app) does: an agent started by hand in
any terminal is recognised, and its tab shows its mark. They also want Claude Code's
pixel mark ("Clawd") instead of the starburst.

Decided with the user, in order:

1. **Design the new detection, do not chase the Mac bug.** The Mac root cause was not
   found (§1, F3); the user chose to design something that does not depend on it.
2. **At a glance the tab says three things:** "se sta lavorando, se aspetta me o se ha
   finito": working, waiting for me, finished.
3. **When an agent finishes it sends a notification.** The mapping onto unread-sessions
   (a stop posts a notification when the session is not on screen and leaves an unread
   mark) was accepted ("va bene").
4. **Icon A: Clawd in orange, no tile.** Options B and C, with a dark tile, were shown and
   not chosen.
5. **Every agent Sirio supports:** Claude Code, Codex, OpenCode, Pi and Oh-My-Pi (omp).
6. **Precise status is required for Codex and OpenCode too** ("indispensabile"), not only
   their icon.
7. **OpenCode gets its status from its own server** (§3.6), accepting that the server's
   API is undocumented and can change.
8. The rest of the play-back was accepted as written ("va bene così"). Every point marked
   *[my call]* below is a decision the user has seen and not changed. It was not asked for.
9. **After an update, the new detection waits for the host to restart** (answer "A").
   Terminals already open when Sirio updates keep today's detection until their host
   ends. The app does not get a second copy of the scanner to cover them. The release
   notes say so (§3.1, §5).

## §1 Facts this design rests on

[read] means the code or a file was read. [run] means it was observed live on Linux on
2026-11-02 with the CLI driven in a PTY against a local fake model API. The probe
scripts are throwaway and not in the repo; §6 asks for their equivalent as a test.

| # | Fact | Evidence |
|---|------|----------|
| F1 | Linux, plain case: a hand-typed `claude` is recognised within 5 s and the icon shows. Layer D (the process walk) sees comm `claude`. | [run] isolated Sirio on Xvfb |
| F2 | Claude 2.1.316's working title is `◐ Claude Code` / `◑ Claude Code`; idle is `✳ Claude Code`. `title.rs::detect_claude` knows only `✳`, `. ` and braille, so a pane whose identity rests on the title loses identity and icon on every working turn and gets them back at idle. | [run] reproduced in Sirio with comm forced to `2.1.316`; [read] `sirio_activity/src/title.rs` |
| F3 | On the Mac: `sirio-host → zsh → claude`, the process name is `claude`, `TMUX` empty, no `DISABLE_TERMINAL_TITLE`, yet `sirioctl panel list --json` reports `agent:""`. Both the process walk and the title fail there. Cause unknown. | [run] the user's output from the Mac |
| F4 | **OSC 7501, the program status protocol** (herdr): `ESC ] 7501 ; state=<idle\|working\|done\|blocked\|error\|clear> : id= : kind=<permission\|question\|auth> : progress=0..100 : app= : title=<b64> : msg=<b64> ST`. A probe `ESC]7501;?ST` asks the terminal whether it listens; the answer is an OSC 7501 whose data starts with `?`. A record with an `id` is a sub-record; the root record has none. herdr counts a turn as ended when the root record leaves `working` for anything. | [read] herdr-core `status.rs`, `tests/m3.rs` |
| F5 | Claude Code 2.1.316 sends the probe at startup. Only when it is answered does it report, with `app=claude-code`: `idle` at once, `working` on a prompt, and `done` at the end of a turn. No `idle` follows the `done`. A tool run adds a second `working` with a `msg` (the tool's description). A permission ask reports `blocked kind=permission`, seen live with a permission rule that asks before Bash; a startup dialog reports `blocked` with no `kind`. Once answered, its title becomes plain `Claude Code`. (The first version of this row said no `done` was seen and a turn ends working → idle; corrected by the build's live run.) | [run] |
| F6 | **Codex 0.166 reports OSC 7501 without any probe:** `idle` at start, `working` on a prompt, `blocked kind=permission` on an approval (its title becomes `[ ! ] Action Required \| <dir>`), `done` at the end of a turn followed by an `idle`, and `idle` after Esc. The `blocked kind=permission` on an approval was seen again in the live run. At startup it also sends `working` then `idle` about 10 ms apart, in most runs and not in a warm start. It sends OSC 21337 too. | [run] |
| F7 | Pi 1.4.0 sends the probe, and once it is answered reports `idle`, `working` on a prompt and `done`, with `app=pi`; its title is `π - <dir>`. omp reports `app=omp` without a probe: `idle` at start, `idle` again at Enter, `working`, `done`. Its idle title is `π > <cwd>` and its working title is `π ⠋ <dir>`, a spinner after the glyph; Sirio's title rule looked for `π:`, so it took omp for pi. (The first version of this row said Pi reported nothing in 8 s; corrected by the build's live run.) | [run]; [read] `sirio_activity/src/title.rs` |
| F8 | **OpenCode 2.0.29 reports nothing:** no 7501, no OSC 9;4 progress, no OSC 99 notification even when its notification query is answered. Its title is `OpenCode`, then `OC \| <session title>` after the first turn. | [run] |
| F9 | **OpenCode v2's terminal UI is a client.** The work runs in one managed background server per user (`opencode serve --service`, default port 49374, set with `opencode service set port`), started once and then detached (parent pid 1). Plugins run in that server, so they never see the pane's environment: today's `SIRIO_PANE_ID` hooks cannot tell panes apart. Its root URL serves a web UI; the event API paths were not found yet. | [run] |
| F10 | All five CLIs are native binaries whose process name equals the command (`claude`, `codex`, `opencode`, `pi`, `omp`). Claude's native install is `~/.local/bin/claude` → `~/.local/share/claude/versions/<version>`. | [run] Linux and the Mac |
| F11 | The host's session engine already calls `tcgetpgrp` on the PTY every loop and publishes a `Foreground { running }` flag (`term.sessions` event `foreground`, `SessionSummary.foreground`). It carries no pid or name. On Windows it is always `false`. | [read] `sirio_term_session/src/owner.rs:337-348`, `protocol/host-v1/schema.json` |
| F12 | Every PTY byte passes through `terminal.vt_write(&bytes); journal.push(&bytes)` in the session engine, which runs in the host and in the in-process `LocalLink` fallback alike. `libghostty-vt` 0.2.2 (ghostty `a887df42`) has no 7501 support and no newer crate exists. | [read] `owner.rs:199,410`, `sirio_terminal/src/link.rs:200` |
| F13 | Today the hooks end a Claude, Codex or omp **turn** with `needs-input`; only a process exit gives `done`. The status model is `Running`, `NeedsInput`, `Done`, `Error`. | [read] `sirio_agents/src/claude.rs:136-141`, `sirio_activity/src/status.rs`; unread-sessions F6 |
| F14 | unread-sessions: a *stop* is a pane going from Running to a settled state (Done, NeedsInput, Error), or a pane whose first status is already settled. A stop posts one notification when the session is not on screen and leaves an unread mark until seen. | [read] unread-sessions design §3–§5 |
| F15 | The Clawd path is already in the repo, monochrome: `rust/assets/icons/chat/claude-code.svg` (LobeHub `claudecode.svg` 1.95.0, MIT, attributed in `rust/assets/icons/chat/ATTRIBUTION.md`). `Icon::ClaudeCode` draws `icons/lobehub/claude-color.svg`, the starburst, on the tinted path with `#D97757`. A comment there says a test keeps tint and asset in step; no such test exists (the chromatic-mark test exempts `ClaudeCode` instead). | [read] `sirio_ui/src/icons.rs:246,313,385-391,846-868` |
| F18 | Windows' own ConPTY swallows the answers a terminal writes to a program, the 7501 probe answer included; herdr ships Microsoft's newer ConPTY for this reason. | [read] herdr-core `scripts/fetch-conpty.ps1` |
| F16 | herdr-app and herdr-core are Apache-2.0; Sirio is MIT. | [read] their `LICENSE` files |
| F17 | herdr names a program from the foreground process **argv**, unwrapping shell wrappers (`zsh -ic '"$@"'`), PowerShell `& 'cmd'` and interpreters (node, bun, npx, uv…, by the script). It shows no status for programs that do not speak 7501 ("do not guess status from screen text"). | [read] herdr-app `src/programs.rs`, herdr-core `SKILL.md` |
| F19 | **Not read on a Mac.** The user skipped the reading on the Mac (2026-11-02). Two things were measured on Linux only: the foreground group equals the terminal's group (pgid == tpgid) for a hand-typed agent, and Claude's probe reaching the host. On macOS they rest on `macos-check.yml` (the host E2E on `macos-15`) alone. Three readings are owed before the release that ships this: (1) the "command running" pill lights; (2) `ps -A -o pid,ppid,pgid,tpgid,tty,comm \| grep -E 'zsh\|claude' \| grep -v grep` shows the `claude` row with pgid == tpgid; (3) `script -q ~/c.typescript claude`, then `/exit`, then `grep -c $'\e]7501;?' ~/c.typescript; rm ~/c.typescript` prints ≥ 1. | The user's call; [run] the Linux half |
| F20 | **Wrapper scripts.** On the dev machine `~/.local/bin/claude` and `~/.local/bin/pi` are bash wrappers that run `mise use -g` and then `exec mise x <cli> -- <cli>`. A hand-typed agent can therefore have a wrapper as its group leader for a moment. An identity from a report's `app=` covers it, and so does the resolver's `mise x … --` form. Running such a wrapper edits the user's global mise config, so no test ever runs one. | [read] |

## §2 Goals, non-goals, anti-goals

**Goals**

- An agent typed by hand in any Sirio terminal is recognised on macOS and Linux, whatever
  its launcher (a symlink, `npx`, `bunx`, `node …/cli.js`), and shows its mark in the tab
  and in the sidebar.
- For all five agents the pane shows *working*, *waiting for you* or *finished*, from what
  the agent itself reports, never from reading the screen.
- A finished or waiting agent produces an unread-sessions stop: a notification when its
  session is off screen, and an unread mark.
- Claude Code is drawn with Clawd, orange, no tile, wherever `Icon::ClaudeCode` is drawn.

**Non-goals**

- Fixing the Mac failure of the current layers in place (F3). The new path replaces the
  layers that fail there. A hand check on the Mac is still owed (§6).
- Detecting programs other than the five agents (herdr's ~120-entry table).
- Showing 7501 progress, sub-records, `msg` or `title` text in the UI.
- Renaming the tab when an agent is recognised: the tab keeps the label it has today,
  only the icon changes. *[my call]*

**Anti-goals** (it fails even if it "works")

- Status guessed from screen text (spinners, progress bars). OpenCode's spinner was
  rejected as too fragile.
- A notification for something that is not the end of a turn: startup, the Codex startup
  blip (F6), a cleared status.
- Writing anything into user-global agent config to make this work. The existing rule
  (`prepare` writes only worktree-local files) stands.
- A process name, argv, `msg` or `title` from a report reaching a log or a perf label
  (the `sirio_perf` and host-log rules: names and ids, never content).

## §3 Design

### §3.1 Who is running: the host names the foreground program

The session engine already knows when a foreground process group is not the shell
(F11). It also reports **which program** that is: the group leader's pid, process name
and argv. It reads them only when the foreground group changes, not every loop, using
`/proc` on Linux and libproc (`proc_name`, `KERN_PROCARGS2`) on macOS. *[my call]*
This goes over the host protocol as a new capability (for example `term.program`, the
next minor), with the `foreground` event and `SessionSummary.foreground` carrying the
program when there is one. It follows the ledger rules of
`2026-10-05-host-foundation-design.md` §6.4 (capability entry, schema, conformance
case). An older host without the capability keeps today's behaviour.

That older host is also the one already running when Sirio updates. The client adopts
any live host of the same major (`sirio_host_client/src/ensure.rs`, `adopt_if_live`) and
replaces one (`retire`) only when it lacks the terminal capability and has no sessions.
So while any terminal stays open, the old host keeps serving every pane, new tabs
included, and the new detection reaches none of them. Closing every tab is not enough
either: the app holds a host connection for its whole life (`ensure.rs` returns it and the
app keeps it), and a host leaves on its own only with no client and no session
(`try_begin_idle_exit`, idle grace 60 s in `sirio_host/src/main.rs`). The new host starts
once the old one has gone: after **End all sessions**, or after every terminal is closed and
Sirio stays closed for over a minute (an update that relaunches within that minute adopts the
old host again). This is accepted (decision 9). There is no app-side fallback that scans the
attach stream and answers the probe. Nothing here retires a host that has sessions (§4).

The app resolves the agent from that argv, not from a walk of the shell's descendants:

- `argv[0]`'s file name, then the resolved executable's file name, against the five
  agent names.
- Interpreters and launchers are unwrapped as herdr does (F17): `node`, `bun`, `deno`,
  `npx`, `bunx`, `pnpm dlx`, `uv run`, `python -m`, and a shell `-c` wrapper. A script
  path counts by its package (`…/node_modules/@anthropic-ai/claude-code/cli.js` →
  Claude Code).

On macOS and Linux, for panes on a host with the capability, this replaces Layer D (the descendant walk
polled every 500 ms) as the source of identity. Layer D stays for the in-process
fallback only if the engine path cannot serve it; the builder checks, since `LocalLink`
runs the same engine (F12). *[my call]* herdr's resolver is Apache-2.0 (F16): port the
behaviour, or the code with its licence notice kept.

**Identity also comes from a report.** A 7501 report's `app=` (`claude-code`, `codex`,
`omp`, and whatever pi sends) names the agent even when the process is not recognised.
That covers Windows, where there is no foreground group (§3.8), and any launcher the
resolver misses. *[my call]*

**Identity must not flicker.** While an agent runs a tool, the foreground group can, in
principle, become one of its children. The pane keeps the agent identity while the agent
process that was recognised is still alive and a descendant chain links the new
foreground to it, or while the agent's 7501 status is live. The builder verifies with the
real CLIs which of these is needed. *[my call]* **Measured:** in every live run of Claude
and Codex running a 3 s shell tool, the foreground group never changed and the program
stayed the agent. [run] The rule is kept for the case where it does (§8, 5).

### §3.2 What it is doing: OSC 7501 in the session engine

The session engine scans every PTY chunk for OSC 7501 before the existing
`vt_write`/`journal.push` (F12), in the host and in `LocalLink` alike, because the host
is where a session lives while the app is closed. *[my call]* The scanner:

- handles a sequence split across reads, both terminators (BEL and ST), and a bounded
  length. A sequence over the bound is dropped, never buffered without limit.
- **answers the probe** `ESC]7501;?ST` by writing an OSC 7501 answer starting with `?`
  to the PTY, once per probe. The exact bytes are herdr's/ghostty's, confirmed against
  the live CLIs (F5: Claude accepted `ESC]7501;?ESC\`).
- keeps the **root record** (no `id`) per session: state, kind, app. Sub-records are
  parsed and dropped (§2 non-goal). `clear` with no id removes the root record.
- publishes a change as a session event under the same new capability, and keeps the
  current record in `SessionSummary`, so an app that attaches or relaunches sees the
  current status without waiting for the next report.

Answering the probe only tells the agent that the terminal listens. Nothing else is
written to the PTY.

### §3.3 From a report to what the user sees

| Report (root record) | Pane status | What the user sees |
|---|---|---|
| `working` | Running | green ring, travelling |
| `blocked` (any `kind`) | NeedsInput | amber ring: *waiting for you* |
| `error` | Error | red ring |
| `done` | Done | green ring, full: *finished* |
| `idle` after a `working` that lasted at least 1 s | Done | green ring, full: *finished* |
| `idle` after a `blocked` that came inside a turn (for example a rejected permission) | Done | green ring, full. NeedsInput → Done is not a new stop, so no second notification |
| `idle` after a turn too short to count (the 10 ms Codex blip, a startup dialog that was answered), `idle` with no turn open and no status shown (startup), `clear` | no status | the agent's mark alone |
| `idle` with no turn open, after a `done` or an `error` (Codex sends one right after every `done`) | the status stays | *finished* or the red ring, until the next `working` or `blocked` |
| `idle` with no turn open, after a `blocked` that came outside any turn | no status | the agent's mark alone |

- **A turn** starts at `working` and ends at the first `idle`, `done` or `error`. A
  `blocked` spell inside it does not end it. *[my call]*
- Every settled state reached from Running is an unread-sessions stop (F14): a
  notification when the session is off screen, an unread mark, and one notification per
  stop however many sources report it.
- **An agent that exits mid-turn** (the foreground returns to the shell while the pane is
  Running) settles to Done first, which is a stop, and then its identity and status
  clear. *[my call]*
- The 1 s threshold is what separates a turn from Codex's startup blip (F6). An Esc that
  interrupts a real turn shows *finished*. *[my call]* If live testing finds a cleaner
  signal, the builder may replace the threshold. The anti-goal stays.
- *Waiting for you* (NeedsInput) is only `blocked`: a permission, a question, a sign-in.
  The end of a turn is *finished* (Done). This is how the user's three states stay
  distinct. Today's hooks collapse them (F13). *[my call]*

### §3.4 Which source wins

Per pane, from strongest to weakest:

1. **OSC 7501** (or, for OpenCode, its server: §3.6). While a pane has a live root
   record, hook `notify` calls no longer set that pane's status, `SessionEnd` included
   (an exit is handled by the rule in §3.3). They keep every other job, such as the
   session ref. *[my call]* Without this, a Claude spawned by Sirio
   would show *waiting for you* (its `Stop` hook) and *finished* (its 7501 idle) for the
   same moment.
2. **Hooks**, for agents and versions that send no reports.
3. **Title** (Layer B), the fallback for versions without 7501. It is fixed: Claude's
   `◐`/`◑` are working glyphs (F2), and omp's `π > ` is omp, not pi (F7).
4. **Process exit code**, as today.

**Below 7501, nothing changes.** Hooks and titles keep today's mapping, where the end of
a turn shows *waiting for you* (amber, F13). They still produce a stop and its
notification. The three states are told apart only where the agent reports: 7501, or
OpenCode's server. *[my call]* Pi is checked first (§7). If pi sends no reports, pi is
the one agent where *finished* and *waiting* are not told apart, and the PR records this
as a known gap.

Ownership keeps its current meaning
(`docs/agents/architecture/activity-detection.md`). A pane recognised through §3.1 or
§3.2 is owned by that evidence. **When the foreground returns to the shell** (the agent
exited), identity and status clear at once, after the agent's last report is applied, so
an exit mid-turn still produces its stop. An unread mark already set stays until seen
(unread-sessions).

### §3.5 Tabs, sidebar, notifications

A hand-started agent behaves like one Sirio launched while its tab is attached: same mark in
the tab (`tab_agent_mark` / `tab_icon`, `main.rs:12537`, `:4884`) and in the sidebar row, same
rings, same unread-sessions stops. *[my call]* **The limit.** A hand-typed agent's program and
status reach the app only through its tab's attached terminal. After a relaunch only the tab
on screen attaches (the others attach as they are shown), and an evicted worktree's tabs
detach (`docs/agents/architecture/panes.md`). Until a tab attaches, its turns make no stop and
no unread mark, while a Sirio-launched agent in the same place still notifies through its
hooks. Today `tab_agent_mark` falls back to the
activity model only for a tab's primary pane; a split tab follows unread-sessions F2
(the most urgent pane decides the status). The tab label does not change (§2).
`sirioctl panel list` reports the recognised agent in its `agent` field, which is how
the E2E reads it.

### §3.6 OpenCode: listen to its server

OpenCode reports nothing in the terminal (F8). Its v2 work happens in the user's
managed background server (F9), which is also what the hand-typed `opencode` talks to.
Sirio:

- finds that server through its registration file, the one `opencode` itself reads:
  `$XDG_STATE_HOME/opencode/service.json` (else `~/.local/state/opencode/service.json`),
  `{id, version, url, pid, password}`, mode 600. Sirio reads the file (no network call,
  no environment variable: the service ignores `OPENCODE_SERVER_PASSWORD`, so an app started
  from the Dock finds it too) and its watch thread confirms the service with
  `GET <url>/api/info` (Basic `opencode:<password>`), accepting it only when the reply's
  `pid` is the file's. The file is read again on every reconnect: a restarted service has a
  new port, password and pid. [run]
- reads the state before the stream from REST (`GET /api/session?directory=`, an exact
  match on the pane's canonical cwd; `GET /api/session/active`; `GET
  /api/session/{id}/permission` and `/form`) and subscribes to `GET /api/event` (SSE,
  `data:` frames, a `server.connected` first, nothing replayed). Per session: busy on
  `session.execution.started`; done on `.succeeded` and `.interrupted` (the service's own end of
  a turn is a finish however short it was; a session the list reports as not running is idle,
  no status); error on `.failed`;
  waiting on `permission.asked` and `form.created`, busy again on `permission.replied`,
  `permission.rejected`, `form.replied` and `form.cancelled`. `session.execution.*` carry
  no `location`, so they map through `data.sessionID`; the title comes from
  `session.renamed`. [run] (`session.status` and `session.idle` are
  not emitted by v2.)
- links a server session to a pane only when it is sure. The pane's foreground is
  `opencode` (§3.1), the session's directory is the pane's cwd, and the session's title
  equals the pane title after `OC | `. A session has no title when it is created (at the
  first prompt): the service names it with a separate `session.renamed` a moment later,
  sometimes after the first turn ended and sometimes during it [run], and the TUI's title
  follows that rename. So until then a pane links to the one session in its cwd that was
  created after the pane's `opencode` started, provided **it is the only pane running
  `opencode` in that cwd**; and a pane that is linked keeps its session when its `OC | <t>`
  title arrives before the rename does, while no titled session in the folder matches it, and
  while the held session is in the folder and has the pane's title even if an older session
  of the folder has the same one (the held one wins).
  Otherwise it waits for the title. If two panes match, or none, the pane shows the
  OpenCode mark and **no status**: no guessing (anti-goal). A reconnect or a relaunch
  restores Busy and Waiting from REST, but not a finished or failed result: the seed does
  not read `outcome`, so an idle session shows no status until its next event.
- maps busy → Running, waiting → NeedsInput, idle after busy → Done, an error →
  Error, the same table as §3.3.

This depends on an undocumented API, accepted by the user (§0, 7). Like the Claude
stdio protocol (`docs/agents/architecture/claude-chat.md`), it **degrades rather than
fails**: no server, a refused connection, a stream that ends or goes quiet or a reply it
does not understand leaves the OpenCode mark with no status, and is logged once by name per
run of the app, never with content. An event name it does not know is ignored without a
log, and the service's version is not read.
OpenCode 1.x (no managed service) keeps today's behaviour. The native chat transports
plan (`docs/native-chat-transports`, PR #691) runs *Sirio's own* OpenCode server; this
one is the user's. They are different servers, and they may share the client code for
the event stream if that work has landed first. Neither waits on the other.

### §3.7 The Clawd icon

`Icon::ClaudeCode` draws Clawd: the LobeHub `claudecode.svg` path already in
`rust/assets/icons/chat/claude-code.svg` (F15), in `#D97757`, with **no tile**, in
both appearances. It replaces the starburst wherever the icon stands for **the agent**:
tabs, sidebar rows, the agent catalog and the menus that start an agent, and not only
for hand-started agents. *[my call]* Where it stands for **the Claude account or
provider**, the starburst stays: the Settings account card (`ProviderKind::icon`,
`sirio_ui/src/settings.rs`) and the status bar's Claude usage segment
(`sirio_ui/src/status_bar.rs:691`). Those surfaces get their own icon variant. The chat's
own Clawd (`chat/identity.rs`, monochrome, theme-coloured) is unchanged. *[my call]*
The agent mark stays on the tinted path with the same hex. Clawd's asset is monochrome
(`fill="currentColor"`), so the tint is its only colour. The exemption that
`every_chromatic_mark_declares_its_own_colours` gives `ClaudeCode` today moves to the
account variant, which keeps the starburst and its baked fill (F15 corrects the earlier
claim of a tint test). The attribution is already there. herdr's `claude.svg`
is the same path and adds nothing to attribute.

### §3.8 Platforms

- **macOS**, the reference platform, gets all of it: `tcgetpgrp` and libproc in the host,
  7501, OpenCode's server. The Mac failure (F3) is not reproduced on Linux, so the E2E
  cannot prove the fix there; the macOS hand check (§6) can.
- **Linux** gets all of it.
- **Windows**: ConPTY has no foreground process group (F11), so the host names no program
  there and Layer D keeps identifying agents as it does today. A 7501 `app=` and
  OpenCode's server add identity on top. The scanner runs the same, but Windows' own
  ConPTY swallows the probe answer (F18). A hand-typed Claude, which reports only once
  answered, therefore gets no 7501 status on Windows. Codex and omp report unprompted
  and get theirs, provided ConPTY passes their reports through, which is unverified.
  Shipping another ConPTY is out of scope. *[correction made while planning,
  2026-11-02]*

## §4 Constraints

- Builds on `main` after unread-sessions merges. Stops, notifications and unread marks
  are its, not re-implemented here.
- The host protocol rules: a new capability, never a changed meaning of an existing
  field; an older host or client degrades to today.
- Nothing kills or restarts a host for this. The host only gains a scanner and a
  process read.
- No new user-global writes, no new control-socket method that writes. If the builder
  adds a read verb (for example to expose program status), it goes through
  `served::served_methods` like the rest.
- Logs and perf labels: program names and ids only (§2 anti-goal).
- `Scripts/ci.sh` / `Scripts/ci-linux.sh` are run only when the user asks. Iterate with
  `cargo build/test -p <crate>`.
- Version: the first `feat:` of the cycle moves the version from patch to minor, per
  CLAUDE.md.

## §5 Docs to update with the change

- `docs/agents/architecture/activity-detection.md`: the new evidence (foreground program,
  7501, OpenCode server), the order in §3.4, ownership.
- `docs/agents/architecture/host.md`: the new capability, the probe answer, the record
  kept per session.
- `protocol/host-v1/`: capability ledger, schema, conformance case.
- `docs/agents/e2e-tests.md`: the new E2E (§6).
- The release notes of the version that ships it carry five lines:
  - agents typed by hand are recognised once the old host has gone: after **End all
    sessions**, or after every terminal is closed and Sirio stays closed for over a minute
    (decision 9; closing the tabs of a running Sirio is not enough);
  - in a terminal tab whose session has named a program, an agent title that arrives before
    the program is named gives the tab its agent mark only, and the first 2 seconds set no
    status from titles or screen (§9, 5);
  - OpenCode's status needs OpenCode v2's background service; with 1.x, or with no service
    running, the mark shows without a status (§3.6);
  - a hand-typed agent's turns make a stop and an unread mark only while its tab is attached:
    after a relaunch that is the tab on screen, and the others as they are shown, while a
    Sirio-launched agent in an unattached tab still notifies through its hooks (§3.5);
  - Sirio-launched Claude, Codex and omp tabs show *finished* at a turn's end, instead of
    *waiting for you*, once the host reads their reports (M9: their Stop hook used to say
    *waiting for you*).

## §6 How it is proven

**End to end, with an artifact** (the shape of `Scripts/Tests/test-host-sessions-e2e.sh`):
`Scripts/Tests/test-hand-agents-e2e.sh` drives a real `sirio`, `sirio-host` and `sirioctl`
under Xvfb with an isolated root, and prints `HAND AGENTS E2E OK (25 cases, <s>s)`. One stub,
`Scripts/Tests/stubs/hand-agents/agent.py`, plays `claude`, `codex`, `omp` and `pi` on
`PATH`, and behaves like the measured CLIs: the stub `claude` sends the probe and reports
only when answered, the stub `codex` reports unprompted, including the 10 ms startup blip.
One stub is launched through a `node`-style wrapper. The script types them into plain
terminal tabs, records `sirioctl panel list --json` at each state, captures PID-matched
window frames (the Clawd mark, the three rings), checks the notification and unread
mark, and checks that leaving the agent clears the mark. OpenCode reports nothing in the
terminal, so its stub is a server: `Scripts/Tests/stubs/opencode-server.py` speaks the
real v2 endpoints and the events of §3.6. The host side is cases 16 to 27 of
`Scripts/Tests/test-host-e2e.sh`, the script `macos-check.yml` runs on `macos-15`.

**Opt-in live tests** (like `sirio_agents`' `hooks_live`): the real CLIs against local fake
model APIs, recording the 7501 sequences and OpenCode's events. They are `program_status_live`
in `sirio_term_session` (the real `claude`, `codex`, `omp` and `pi`, typed into a session of the
engine) and `service_live` in `sirio_opencode_watch` (the real OpenCode v2 service), and the run
lines are in `CLAUDE.md`. This is where F5–F9 are re-proven when an agent updates. They refuse
to run with `TMPDIR` inside the user's home (agents read instruction files from every ancestor
directory), resolve `claude` and `pi` to the newest real mise install, refuse a mise wrapper,
and stop the private OpenCode service only after checking that the process carries the private
`HOME`.

**Unit tests, failure modes first**, only for the two units that stand alone: the 7501
scanner (split reads, BEL/ST, over-long, malformed fields, a probe inside other output,
a probe answered once) and the argv resolver (each wrapper of §3.1, a symlinked native
install, a renamed binary). Write the failure list before the code.

**Mac, before building.** The plan's first task was a reading on the user's Mac: `claude`
running in a plain tab, the session's `foreground` from the host, and whether the PTY
output contains `ESC]7501;?`. The user skipped it (2026-11-02), so the readings were not
made before building. They became the three readings of F19, owed before the release that
ships this, and until then the host side on macOS rests on the CI case below.

**Mac in CI.** The host conformance case for the new capability (the probe answered, a
report kept, the foreground program named) runs in the host E2E, which
`macos-check.yml` already runs on `macos-15`. That proves the host side on macOS for
every PR.

**Owed by hand:** the three readings of F19, and then, on the user's Mac with an installed
build, type `claude`, `codex` and `opencode` into a plain tab: mark, three states,
notification. Record the result in the PR.

## §7 Left to the builder

- OpenCode v2's event API: paths, authentication, event names and payloads, how to
  read the service port. Found by the live test before the code that depends on it.
- The exact bytes of the probe answer, from herdr-core's ghostty and the live CLIs.
- What pi reports, if anything, once answered.
- Whether the foreground changes while an agent runs a tool (§3.1), and so how much of
  the "no flicker" rule is needed. Answered: it did not change in any live run (§3.1).
- Where the 1 s threshold lives, or a cleaner turn signal (§3.3).
- The exact names of the capability, events and fields.
- How the host reads argv on macOS without new permissions (libproc and `sysctl` need
  none for the user's own processes).

## §8 Decided while planning

Approved by the user on 2026-11-02 ("va bene così"), with the plan. Where they differ
from §3, these win.

1. **Windows** keeps Layer D for identity (§3.8, corrected above). An identity that came
   from a 7501 `app=` clears with its record, on the shell's exit, or when Layer D finds
   no agent process.
2. **A `blocked` before any turn** (a startup dialog) shows amber and is a stop, so it
   notifies when the session is off screen (F14). The anti-goal "no notification at
   startup" covers the startup `idle`, the Codex blip and startup titles.
3. **The turn clock is the host's.** Each record carries the time the host saw its state
   (ms since the session started), in events and in `SessionSummary` alike. A turn's
   length is measured between two such times, so neither an app stall nor a relaunch
   changes it.
4. **argv on the wire** is capped at 32 arguments of at most 1024 bytes. The program's
   cwd travels with it, for OpenCode's linking (§3.6). Neither is ever logged.
5. **No flicker** (§3.1) is one rule: a program-owned pane keeps its agent while a 7501
   record or an OpenCode link is live. The descendant-chain rule is built only if the
   live facts show the foreground group changing during a tool run; they did not, so it
   is not built.
6. **The OpenCode watch** runs only while some pane's foreground program is `opencode`.
   It reconnects with back-off (2 s doubling to 60 s), logs each failure kind once per
   run, and ignores proxy variables.
7. **Startup titles are silent.** On a program-owned pane, title and screen content set
   no status for 2 s after the agent is recognised, and none while a record is live.
   After that an agent that never reports (pi, an older Claude) gets today's title
   mapping. Clearing a record also clears the title status under it.
8. **Layer D stops for a pane once its session has named a program.** Tabs without job
   control (a `--cmd` tab, a Sirio-spawned `$SHELL -lc`) never name one and keep it.
9. **The probe is answered at most once per second per session,** in byte order with the
   emulator's own replies. An echoed answer cannot start a loop.
10. **The host E2E proves the protocol's behaviour.** A conformance case cannot wait for
    output and skips events, so conformance pins only the Welcome capability array.
11. **The resolver tries the process name after argv** (node's `process.title`). No
    executable path is carried; a native install's file name is its version (F10).
12. **The host re-reads the foreground program** after a program group changes, every 250 ms
    until two reads agree (the child takes the terminal before it `exec`s), then every 2 s
    while it stays in the foreground, and names it again only when a read differs.
    - When the shell is back in the foreground for two polls, the host sends "no program"
      and clears the record, then drops reports for 1 s. It does not drop them until the
      foreground changes again: a report that late is bytes written just before the exit,
      and an `exec claude` that keeps the shell's group must be heard.
    - When a named leader has died and another group takes the terminal, the host first ends
      the old program (no program, no record), then names the new one.
    - When a child group took the terminal from a live program and gives it back, it is the
      same program: the record is kept.
    - On the shell's exit the record is dropped from the summary, and the app settles a turn
      that was mid-way (§9, 3).

## §9 Decided while building

Approved by nobody yet: these are Claude's calls, made during execution. Each one is
marked *[my call]* and is open to the user.

1. **Stale reports.** The app ignores a report older than the one it holds, by the host's
   clock. A "no record" always applies. After a reconnect the app is re-seeded from the
   host's summary, and the events queued while the connection was down replay after the
   seed. The user sees no tab flip back to *working* after a turn ended. Without this, a
   replayed `working` would reopen a finished turn and notify twice. *[my call]*
2. **Hooks yield only to a record that shows.** A record on a pane with no agent identity
   does not swallow that pane's hooks. A report's `app=` names the agent in any tab whose
   program the resolver misses (a wrapper, F20), not only where no program is named. The
   user sees the mark and the status in a tab started through a launcher the resolver does
   not know, and a hook is never dropped for a record nobody can see. A stray `app=` from a
   program that is no agent can name a tab until the program ends. *[my call]*
3. **A record lives as long as its session.** When the session's child exits or the host is
   lost, a turn that was mid-way settles to Done, which is a stop, and then the record goes.
   A spawned tab's exit code then shows. A failed exit of a Sirio-spawned agent tab mid-turn
   is one stop (Error), not Done and then Error. A lost host also clears a hand-typed
   agent's mark. A new session starts its clock again near zero, so a kept record would
   treat every report of it as a replay. *[my call]*
4. **Windows.** An identity that came from `app=` clears when its record clears, on exit, or
   when Layer D, having seen the agent's process in that tab, sees none. Layer D cannot see
   Node-hosted agents, so for those only the first two apply. This is untested on Windows
   (below). *[my call]*
5. **The first title of a program session.** In a tab whose session has named a program, an
   agent title that arrives before the program event naming the agent gives the identity
   only, and the 2 s startup grace starts. Otherwise a startup title would make a startup
   notification, which the anti-goals forbid. This is a change to the title layer for such
   tabs, and it needs a release-notes line (§5). *[my call]*
6. **Titles never clear an identity a record shows.** A title that stops looking like the
   agent's, while the agent still reports, does not take its mark away. *[my call]*
7. **OpenCode.** *[my call]*
   - A tab whose agent is OpenCode ignores the host's report record, which is always empty
     and would wipe the status the server gave it.
   - After a relaunch, the host's last title of the session reaches the link without passing
     through the title layer, so a running TUI is linked again and no tab is renamed or
     notified by it.
   - A first-turn link survives the moment when the TUI's `OC | <title>` arrives a few
     milliseconds before the service's rename, so the first turn's result is not wiped.
   - A failed per-session permission or question lookup means "not waiting", so one missing
     endpoint cannot keep a stream open and leak a thread per retry.
   - Each failure kind is logged once per app run, by name and never with content.
8. **Known limits, left as they are.**
   - A crash mid-turn reads "finished", per §3.3.
   - After a relaunch, a Codex tab whose last turn ended shows no status: its last report
     is `idle`, which is what the host's record holds, so the seed has no finish to show.
   - A hand-typed agent's program and status reach the app only through its tab's attached
     terminal (§3.5): until a tab attaches (after a relaunch only the tab on screen does, and
     an evicted worktree's tabs detach), its turns make no stop and no unread mark.
   - Another OpenCode client's session created in the same folder after the pane started can
     link to that pane by the first-turn rule, which counts only Sirio's panes (M8).
   - Ctrl+Z on a working agent reads *finished* (the shell has the terminal), and after `fg`
     there is no status until the agent's next report (M11).
   - An older Sirio on a host this version started loses Claude's title status: Claude drops
     its title once the host answers its probe, and an older app cannot read 7501 (critic 6).
   - Under `ssh` the first remote `app=` names the tab for the whole connection, and a remote
     crash mid-turn stays running until ssh exits (critic 10).
   - Replaying a recorded agent session in a plain terminal (`cat`, `less -r`) shows the
     agent's mark and can notify (critic 11).
   - After a reconnect or relaunch OpenCode restores busy and waiting, but not a finished or
     failed result.
   - A subagent's child session can keep OpenCode's first-turn link from forming until the
     session has a title.
   - An OpenCode tab whose cwd the host cannot read is not counted by the "only tab in that
     folder" rule.
   - The Windows paths compile on Linux and are not run.

9. **`idle` keeps a finish.** An `idle` with no turn open leaves a Done or an Error as it is,
   clears a NeedsInput that came outside any turn, and leaves no status as none. Codex 0.166
   sends an `idle` right after every `done` (measured live), which used to wipe the finish
   milliseconds after the stop. omp's `idle` at Enter, followed by its next `working`, still
   works. *[my call]*
10. **The host sends a status once per change.** A report that changes neither the record
    (state, `kind`, `app`) nor its `at_ms` sends no event, so a program repeating `working`
    per tool, or a `cat` of a log full of reports, is not a frame to every subscriber and a
    repaint per report. *[my call]*
11. **Layer D losing an agent takes its record with it.** It settles a turn that was mid-way
    (a stop), drains it, drops the record and only then clears the identity, the order of an
    exit, so the next agent named in that session starts with no record. A `Program(None)`
    settles and clears only for a session that named a program: a reconnect's re-seed sends
    one for a session that never did and must end nothing. *[my call]*
12. **OpenCode's explicit end is a finish.** `session.execution.succeeded` and `.interrupted`
    are Done, so the one-second floor does not hide a short turn; `.failed` is Error. A
    session the list reports as not running stays Idle. The watch follows no redirect, and a
    program's argv in a listing is bounded to 4096 bytes in all. *[my call]*

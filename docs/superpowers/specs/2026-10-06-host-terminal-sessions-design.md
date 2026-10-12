# Terminal sessions in the host — SP2

**Date:** 2026-10-06
**Status:** implemented (PR pending)
**Programme:** SP2 of seven. The programme, its decisions and SP1 (the host
foundation) are `docs/superpowers/specs/2026-10-05-host-foundation-design.md`
— referred to below as *SP1* with its section numbers. SP2 moves every PTY
Sirio owns into `sirio-host`, so that a terminal and the agent in it survive
a crash, a Force Quit, a Quit or an update of the app.
**Prior art:** unpeel's core keeps one resident `libghostty-vt` terminal per
session and answers an attach with the formatter's VT reconstruction of the
screen (`unpeel-core/src/ghostty_vt.rs`, `vt_state_snapshot`); SP2 takes that
shape.

## §0 Intent

Today the gpui window owns every terminal. `TerminalView`
(`sirio_terminal/src/lib.rs`) spawns its PTY through `portable-pty` on first
render, feeds it to a `libghostty-vt` terminal held by a per-pane owner
thread, and kills the pane's process groups on `shutdown` or `Drop`.
`sirio_control::PaneRegistry` owns a second, unrelated set of PTYs for
`sirioctl panel create` — hand-rolled `openpty`/`fork` on unix, raw bytes, no
VT, `Unsupported` on Windows. On relaunch nothing survives: a fresh shell is
spawned and the persisted scrollback text is fed into it.

SP2 makes the host the one owner of PTYs. Success:

- on Linux, macOS and Windows a process started in a terminal tab is the
  **same process** (same pid) after the app is killed with SIGKILL, Force
  Quit, or quit, and relaunched; the tab shows its screen as it was;
- `sirioctl panel.*` works whether the app is running or not;
- an agent can read any terminal's content — a tab's or a `sirioctl` pane's —
  through `panel.read` in both cases.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| No host available | A **temporary in-process fallback**: the terminal runs on the same session engine inside the app, does not survive, and says so in the tab. The fallback is removed once a stable release has shipped the host with no report against it (§10). This supersedes SP1 §7's provisional "fail closed, no local fallback". |
| What ends a session | Closing its tab, its worktree or its project, or the explicit **End all sessions** action. **Quit detaches and nothing else** — an idle shell lives on in the host until its tab is closed. |
| `sirioctl panel create` panes | Host sessions like any other, **invisible by default**; a **Show session** command attaches a tab to one. |
| Who serves `panel.*` | **The host**, so `sirioctl` works with the app closed; the app's control socket forwards `panel.*` to it. Only this slice of SP4 moves forward. |
| Where the VT lives | **Two VTs** (approach A): the host's is authoritative (snapshot on attach, `panel.read`, terminal query replies, foreground state); the app keeps a replica fed by the live stream, and renders and encodes input from it. A bounded byte journal sits on top. Rejected: one VT in the host with the app drawing remote grids (a socket round trip per changed frame, most of `sirio_terminal` rewritten); bytes only in the host (cannot serve `panel.read` with the app closed, loses modes once the ring wraps). |

## §2 Crates

```
sirio_term_session  (-> libghostty-vt, portable-pty, sirio_perf; no gpui)
sirio_host          (-> sirio_ipc, sirio_host_protocol, sirio_term_session)
sirio_host_client   (-> sirio_ipc, sirio_host_protocol)
sirio_terminal      (-> sirio_host_client, sirio_term_session, as today otherwise)
sirio_control       (-> sirio_host_client; PaneRegistry loses its own PTYs)
```

**`sirio_term_session`** is the session engine: what `sirio_terminal`'s owner
thread does today, minus drawing. Per session:

- spawn through `portable-pty` (ConPTY on Windows), with the environment
  `TerminalHandle::new_with_pane_id` sets today (`TERM`, `COLORTERM`,
  `PI_FORCE_IMAGE_PROTOCOL`, `SIRIO_PANE_ID`, `TILLER_PANE_ID`);
- a resident `libghostty-vt` terminal with today's 10 000-line scrollback;
- the replies to terminal queries (`on_pty_write`) written back to the PTY —
  **only here**, attached or not;
- the byte journal (§3.4);
- the OSC title, the foreground state (`tcgetpgrp` on the master on unix),
  the exit status;
- text rendering for `term.read`, and the VT snapshot for an attach;
- key encoding against the VT's modes (for `panel.key`);
- termination: SIGTERM to every descendant process group, SIGKILL after
  500 ms (today's `terminate_descendant_process_groups`), and on Windows a
  job object per session (§6).

Each session keeps today's threading — one owner thread holding the `!Send`
VT, one reader thread on the master. This settles SP1 §11's "one reactor or
per session": per session; Sirio runs tens of sessions, not thousands.

**Two users of the engine.** `sirio_host` keeps `SessionId → Session` and
serves it over the protocol (§3). The app's fallback (§1) runs the **same
engine** in-process. `TerminalView` talks to a `SessionLink` trait with two
implementations, `HostLink` (the socket, in `sirio_host_client`) and
`LocalLink` (a channel to an in-process engine). Removing the fallback later
deletes `LocalLink` and nothing else.

**`sirio_terminal` becomes a client.** Its owner thread stays, with the
replica VT, the key and mouse encoders and the `Snapshot` that `prepaint`
reads in-process; it takes bytes from the link instead of a PTY, sends input
to the link, and **discards** its replica's query replies. Everything that
read process state from inside the app — `shell_pid`, `is_command_running`'s
`tcgetpgrp` on the raw master fd, `exit_status` — reads the link's session
state instead.

**Build.** `sirio-host` now links `libghostty-vt`, so building the host needs
Zig 0.15.2 like the app does; `windows-check.yml` installs it.

## §3 Protocol

Everything in this section is the capability **`terminal`**, protocol minor
**1.1**. No major changes: a client sends a `Data` frame (kind 4, reserved and
refused in 1.0, SP1 §6.1) only after `Welcome` named `terminal` (SP1 §6.4
rule 3), so a 1.0 host never receives one.

### §3.1 Methods

Methods with an effect carry `generation` (SP1 §6.2).

| Method | Effect | Params → result |
|---|---|---|
| `term.create` | yes | `{session_id?, cwd, command: {type: "shell"} \| {type: "program", program, args}, env: {..}, size: {cols, rows}, origin: "app" \| "control", parent?: session_id, prelude?: base64}` → `{session_id, pid}`. `prelude` is fed into the VT and the journal before the child's first byte (a restored tab's scrollback) |
| `term.attach` | yes | `{session_id, resume_from?: u64}` → `{stream: u32, size, mode: "snapshot" \| "resume"}` |
| `term.detach` | yes | `{stream}` → `{}`; the session continues. Closing the connection detaches all its streams |
| `term.resize` | yes | `{session_id, cols, rows}` → `{}`; the last resize wins (several clients: SP7) |
| `term.read` | no | `{session_id, format?: "text" \| "raw", max_bytes?}` → `{data}` (base64 for `raw`); `text` is the VT's screen and scrollback, `raw` the journal's tail. The default format follows the origin (§5) |
| `term.key` | yes | `{session_id, key}` → `{}`; encoded by the host against its VT's modes, with `panel.key`'s key names |
| `term.write` | yes | `{session_id, data}` → `{}`; for clients without a stream (`sirioctl`) |
| `term.feed` | yes | `{session_id, data}` → `{}`; bytes into the VT and the journal, **not** the PTY — what `TerminalView`'s `clear_screen` and `replay_scrollback` do to their own VT today, now done where every replica sees it |
| `term.wait` | no | `{session_id, timeout_ms}` → `{exit: status \| null}` |
| `term.close` | yes | `{session_id}` → `{}`; terminates (§2) and forgets the session |
| `term.list` | no | → `[{session_id, pid, cwd, title, size, exit, foreground: {running}, origin, parent, clients, created_at}]` |

`session_id` is named by its creator (§4.1). The app passes its pane key,
`pane-<n>`; a creator that passes none (`sirioctl`) gets
`pane-c<12 lower-case hex digits>` from the host, a namespace the app's
decimal keys cannot reach. A malformed id, or one the host already holds, is
refused with `invalid_params`. The `pane-` prefix keeps today's id shape for
scripts and for `sirioctl` users. The id is the value of `SIRIO_PANE_ID` in
the session's environment.

**End all sessions** is SP1's `host.shutdown {force: true}`: it terminates
every session and the host. The next terminal the app opens starts a new one.

New error codes, closed on the host and open on the client (SP1 §6.2):
`unknown_session`, `session_exited` (input to an exited session),
`spawn_failed {detail}`, `invalid_cwd`.

### §3.2 Topic `term.sessions`

Events `{type: "created" | "exited" | "title" | "foreground" | "closed", session_id, ..}`:
`exited {status}`, `title {title}`, `foreground {running}`. They replace what
the app reads on its own today — the tab title (Layer B's input), the
"Running" pill, `ChildExited`. Layer D keeps walking the shell's children
from the app with the pid from `term.list`: host and app share a machine
until SP7.

### §3.3 The `Data` frame

This settles SP1 §11's stream-id header. Payload:

```
[ stream u32 BE ][ phase u8 ][ offset u64 BE ][ bytes ]
```

| `phase` | Direction | `offset` |
|---|---|---|
| `0` snapshot | host → client | 0 |
| `1` output | host → client | journal offset of the first byte |
| `2` input | client → host | 0 |

Any other `phase`, a frame for a stream the connection does not hold, or a
`phase` in the wrong direction is a `frame_error` and closes the connection.
The 1 MiB payload limit (SP1 §6.1) applies to `Data` too: a large snapshot is
several consecutive `phase 0` frames, and the first `phase 1` frame ends it.
Input travels as `Data`, not JSON, because typing is the hot path.

### §3.4 Journal, resume and back-pressure

- **Journal.** A ring of the last **4 MiB** of PTY output per session,
  addressed by a monotonic byte offset since the session started.
- **Attach.** With `resume_from` inside the ring, the host replays from that
  offset (`mode: resume`, no snapshot). Otherwise — absent, wrapped out, or
  ahead of the head — it sends the formatter's VT reconstruction: content
  with styles and hyperlinks, modes, scrolling region, tab stops, keyboard
  flags (kitty included), charsets, cursor, and the scrollback; **not** the
  palette nor OSC 7, which belong to the client (unpeel's
  `vt_state_snapshot`). Snapshot and live output are taken atomically on the
  owner thread, so no byte falls between them.
- **The alternate screen.** The formatter reconstructs the *active* screen.
  A client that attaches while the alternate screen is active (an agent's
  TUI, `vim`) receives that screen; the host then sends that stream a fresh
  snapshot when the alternate screen is left, so the replica's primary screen
  and scrollback are the host's again.
- **Connections.** The host reads a connection on one thread and writes it
  from another, fed by the outbound queues; nothing polls. SP1's 50 ms read
  poll stays only for clients that do not use `terminal`. Splitting a
  connection needs a second handle on the stream: `try_clone` on unix,
  `DuplicateHandle` of the overlapped pipe on Windows.
- **Back-pressure.** The PTY reader never waits for a client. Each stream has
  a bounded outbound queue (**4 MiB**); a client that lets it fill is detached
  with the event `{type: "lagged", stream}` and re-attaches (snapshot). An
  agent is never slowed because a window is stuck in a long frame.

## §4 Lifecycle in the app

### §4.1 Identity

`SIRIO_PANE_ID` is in a shell's environment from birth and cannot change, so
the id must be the same on both sides of a relaunch. The app's pane keys
already are: they live in the persisted pane tree, a restored tab gets its
old leaf ids back, and `next_pane_id` resumes above the largest. **The app
therefore names its sessions `pane-<leaf id>`** — the id it already exports
and that the activity model, the hooks and the control socket already use —
and the host refuses a duplicate. At startup the app raises `next_pane_id`
above every `pane-<n>` the host lists with `origin: app`, so a new pane never
collides with an orphan.

A new terminal tab triggers an immediate save of the session (as a hook write
like `save_session_ref` does), not the next periodic one, so a crash right
after creation rarely leaves an orphan; orphan adoption (§4.2) covers the
window that remains.

### §4.2 Events

| Event | Behaviour |
|---|---|
| New terminal | `term.create` then `term.attach`. No host → `LocalLink`, with a banner in the tab: *This terminal will not survive closing Sirio* and the reason |
| Quit, crash, Force Quit | The connection closes: every stream detaches. `LocalLink` sessions die as today. `on_app_quit` no longer calls `shutdown_terminals` for host sessions; `PaneRegistry::shutdown` no longer ends anything (its PTYs are gone) |
| Relaunch | For each persisted tab with a `session_id` listed by `term.list` → `term.attach` (snapshot). Not listed (host gone, reboot) → today's path: a new shell, the persisted scrollback fed into it, the adapter's `resume_command` when the tab carried an agent |
| Close a tab, worktree or project | `term.close` of its sessions — except a tab opened by **Show session** (§5), whose close detaches |
| Worktree unmounted | `TerminalPaneCache` stays; eviction past its cap now **detaches** instead of killing. Reselecting attaches again. Opening many worktrees no longer kills agents |
| Session exits while no client is attached | The host keeps the exited session with its VT until it is closed: the reopened tab shows the last screen and *Process exited* |
| The host dies while the app runs | Tabs show *Session lost* with **Restart** (a new shell, with the replica's scrollback fed in); `ensure_host` starts a host again |
| Orphans — `origin: app` sessions no tab references | Adopted at relaunch: a tab opens in the worktree whose path contains the session's cwd (the deepest one), else in its project, else in the active worktree. An agent never disappears silently |

### §4.3 Idle exit

SP1 §5.5's "zero sessions" counts **live** sessions only. A host holding only
exited sessions and no client exits after the grace period; their last
screens are lost, and the app still has the scrollback it persisted.

### §4.4 Scrollback persistence

The periodic save keeps capturing from the replica VT (in-process, bounded to
256 KiB per pane as today), so relaunch after a lost host still has text to
feed into a new shell.

## §5 `panel.*` and `sirioctl`

**Finding the host.** `sirioctl` resolves the data root by the same rule as
the app (SP1 §4.3, as amended by §7.1) and connects to `host-v<N>.sock`
through `sirio_host_client`.

**Route: the app first, then the host.** `panel.list` carries `tab`, `title`,
`agent`, `active` — layout the app alone knows until SP5.

- **App running**: `sirioctl` uses the app's control socket as today; the app
  forwards `panel.*` to the host and adds the layout fields.
- **App absent** — the socket path does not exist or the connection is
  refused: `sirioctl` goes to the host. Answers carry the same fields, with
  `tab` and `agent` empty.
- A connection that opens and then does not answer does **not** fall through
  to the host: loss of contact is not evidence of death (SP1 §5).

| Verb | On the host |
|---|---|
| `panel.create`, `panel.split` | `term.create {origin: control}`; `split` records `parent` — the direction is metadata only, these panes are never drawn |
| `panel.write`, `panel.key` | `term.write`, `term.key` |
| `panel.read`, `panel.scrollback` | `term.read`. Default format by origin: **raw bytes for `control`** sessions, **VT text for `app`** sessions — each what it returns today; a new optional `format` param asks for the other |
| `panel.state`, `panel.wait` | `term.list`, `term.wait` |
| `panel.close`, `shutdown_for` | `term.close` |

**Show session** (command palette and `sirioctl panel show <id>`) opens a tab
attached to an existing session. Closing that tab **detaches**: the session
belongs to whoever created it through `sirioctl`.

Everything else on the control socket — `notify`, `session-ref`, hooks,
workspace, browser, surface, `session.transcript` — stays in the app until
SP4 (chat content: SP3). With the app closed, `sirioctl notify` keeps exiting
0 silently, as it does outside a pane today.

## §6 Platforms

Each row is a hypothesis until its probe passes, the way SP1 §5.4 was. The
probes run before the tasks that depend on them, and their rows are recorded
in `docs/testing/host-terminal-probe.md`.

| Platform | Risk | Answer |
|---|---|---|
| Windows | A ConPTY child started by the host inherits the host's handles — SP1's `Command::spawn` lesson | Probe what `portable-pty`'s ConPTY spawn passes. If it inherits, spawn with a `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` as `sirio_host_client::detach` does |
| Windows | Killing descendants is a no-op today | A **job object per session** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; `term.close` closes it. Nested jobs (Windows 8+) make it work inside a `WindowsNoBreakaway` host too |
| Windows | ConPTY inside a `DETACHED_PROCESS` host with no console | Probe; `CreatePseudoConsole` does not require a console on paper |
| Windows | The "Running" pill is always false today | The host reports `foreground.running` from the ConPTY's process list if the probe finds a reliable source; otherwise it stays false, as today |
| macOS | **TCC**: agents become children of a launchd job, not of Sirio.app; privacy prompts may name `sirio-host`, and permissions granted to Sirio may not apply | A probe with `sirio_privacy`'s `permissions` example run inside a host session, on the runner and on a real Mac. If attribution moves, the Settings row and the docs say so, and the host binary is signed under an identifier that names it (`app.sirioai.sirio.host`) so a prompt is recognisable |
| Linux | The host's systemd scope also holds its sessions | Intended: stopping the scope stops everything in it |
| all | PTYs opened by a host that is a session leader (SP1 §5.4, *For SP2*) | `O_NOCTTY` on every master and slave the host opens (§7.1) |

The **macOS Force Quit** row deferred from SP1 is a prerequisite of SP2's
macOS E2E: with sessions it is the case itself. It is run by hand on a real
Mac (SP1 §9.3) and recorded in `docs/testing/host-detach-probe.md`.

## §7 What SP2 must do first

### §7.1 Carried over from SP1

The first task of the plan, before any session exists, because sessions make
each of these dangerous:

1. **Data-root isolation.** A debug build or an isolated Sirio (`SIRIO_DB`,
   `SIRIO_SOCKET`) adopts the user's real host today (SP1's known
   limitation); with SP2 it would list and close the user's terminals. The
   root becomes: `$SIRIO_HOST_HOME` if absolute; else, when `SIRIO_DB` is
   set, a `host` directory beside the database; else the default root, with
   a `-debug` suffix on the directory name in debug builds.
2. **Successor safety.** A leaving host checks its socket's (device, inode),
   which a filesystem can reuse after the root is removed and re-created;
   it also requires the state file's `generation` to match before removing
   the endpoint.
3. **`O_NOCTTY`** on every PTY the host opens.
4. **Deadlines** on host connections: a handshake and every frame write are
   bounded, so a stuck peer cannot hold a thread.
5. `sirio_ipc`'s unix endpoint compares the socket path length with `>` while
   the client refuses at `>=`; one rule, the client's.
6. The early `return 4` after `start.state_file_failed` leaves the endpoint
   behind; it is removed first.
7. The Settings row reads a lock-file **open** error as "a host holds the
   lock"; it is reported as the error it is.

### §7.2 Brought forward from before SP4

A **golden-bytes test of the SRH1 codec** — fixed byte sequences decoded and
re-encoded — and the frame layout, `Data` included, written in
`protocol/README.md`. Conformance cases cannot see codec drift when both
sides change together; SP2 is the first change to the codec since SP1.

## §8 Errors

| Situation | Behaviour |
|---|---|
| Host unavailable at terminal creation | `LocalLink` with the banner and the reason (§4.2) |
| `term.create` fails (`invalid_cwd`, `spawn_failed`) | The tab shows the reason, as `TerminalView::failed` does today |
| `lagged` | The client re-attaches with a snapshot, invisibly |
| `unknown_session` on attach at relaunch | Today's restore path (§4.2) |
| The connection to the host drops while the app runs | Reconnect with `resume_from` per stream; *Session lost* only when `term.list` no longer lists the session or the host is `Absent` (SP1 §5.2) — never on the dropped connection alone |
| `sirioctl` with neither app nor host | Exit non-zero with "Sirio is not running and no host is up" for `panel.*`; `notify` stays silent |
| A Data frame with a bad `phase`, stream or direction | `frame_error`, connection closed (SP1 §6.1) |

## §9 Testing

### §9.1 Units, written failure-first

Only the pure units, each with every way it can fail written before the code:

- **`Data` codec**, golden bytes: header, unknown `phase`, wrong direction,
  over-limit payload, truncated frame, a snapshot across several frames;
- **journal ring**: offset arithmetic, `resume_from` inside, wrapped out and
  ahead of the head, the wrap itself;
- **outbound queue**: the producer never blocks; the full queue yields
  `lagged` once;
- **`sirioctl` route**: socket absent / refused / open-but-silent → app,
  host, wait;
- **orphan adoption**: cwd → worktree with nested worktrees, symlinked paths,
  no match;
- **data-root resolution** (§7.1.1): `SIRIO_HOST_HOME` relative and absolute,
  `SIRIO_DB` set or not, debug and release.

### §9.2 Host end to end — `Scripts/Tests/test-term-e2e.sh` → `TERM E2E OK`

A real host driven by `host_probe` extended with the `term.*` verbs;
`--out-dir DIR` keeps the transcript, every host log, and each case's text
read back:

1. A session printing a counter survives SIGKILL of its client; the counter
   keeps rising.
2. Re-attach with a valid `resume_from` → `resume`, and the bytes continue
   contiguously from the last ones received.
3. Re-attach with an offset outside the ring → snapshot. **Snapshot
   fidelity**: a fresh client-side VT fed only the snapshot renders the same
   `term.read text` as the host's VT — with the alternate screen active
   (`tput smcup`), bracketed paste on, and scrollback beyond one screen.
4. With no client attached, `printf '\e[6n'` inside the session gets exactly
   **one** reply.
5. A client that never reads is detached with `lagged` while the session keeps
   printing.
6. `term.close` ends the shell **and** a backgrounded grandchild (on Windows,
   through the job).
7. `sirioctl panel create/write/read/wait/close` with no app goes to the host;
   `read` returns raw bytes for that `control` session.
8. A session exits with no client: it is kept as exited; with no live session
   and no client the host idles out.
9. Every new conformance case under `protocol/host-v1/conformance/`.

### §9.3 App end to end — `Scripts/Tests/test-host-sessions-e2e.sh` → `HOST SESSIONS E2E OK`

A real, isolated Sirio (`SIRIO_DB`, `SIRIO_SOCKET`, `SIRIO_HOST_HOME`) driven
over its control socket; `--out-dir DIR` keeps the transcript, the host's log,
and PID-matched window captures:

1. **Survival**: a terminal running a long process; SIGKILL of the app;
   relaunch → the **same pid** is alive, the tab is attached, `panel.read`
   shows the content from before; then the same through Quit and relaunch.
   The pid is checked first: a new shell with replayed scrollback looks the
   same on screen.
2. **Ending**: closing the tab ends the session; *End all sessions* ends
   every session and the host.
3. **Fallback**: with a host that cannot start (`SIRIO_HOST_BIN` pointing at
   nothing, debug only), the tab runs on `LocalLink` and shows the banner.
4. **Orphan**: a session created and never saved (the app killed before the
   write) is adopted into a tab.
5. **Show session**: a `sirioctl panel create` session attached through
   `panel show`; closing its tab leaves it running.
6. A window capture of the tab before the kill and after the relaunch.

The artefact also records the **echo latency** of a keystroke through
`HostLink` and through `LocalLink` — a measurement for the record, not a
pass/fail bound.

### §9.4 Platforms

- **Linux**: both E2Es locally and in `pr.yml`.
- **macOS**: `macos-check.yml` runs the host E2E and the app E2E (the runner
  has a GUI session); Force Quit by hand (§6).
- **Windows**: `windows-check.yml` runs the host E2E. The app builds on
  Windows only in the release job, so the app E2E on Windows is a **manual
  run** recorded in `docs/testing/host-terminal-probe.md` — stated, not
  glossed over.

## §10 The fallback's expiry

`LocalLink` exists for the release that introduces host sessions. It is
removed in the first cycle after a stable release that shipped SP2 with no
report against the host's sessions; the removal is a `refactor:` commit that
deletes `LocalLink`, the banner and its E2E case. Until then, every
`LocalLink` session logs why it was chosen, so a report can say.

## §11 Out of scope for SP2

Chats in the host (SP3); notification policy, hooks and `sirioctl notify`
against the host (SP4); the workspace model and the session database in the
host (SP5); several clients resizing one session, remote transports (SP7);
surviving a reboot.

## §12 Open points carried to later sub-projects

- **SP4** decides whether `panel.*` keeps the app-first route once the host
  serves the rest of the control socket.
- **SP5** removes the app-first route's reason: when the host owns tabs and
  worktrees, `panel.list` needs no app.
- **SP7** decides resize arbitration between several clients, and whether a
  remote client gets snapshots only (no resume) on a high-latency link.

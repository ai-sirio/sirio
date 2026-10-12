# Probing how terminal sessions behave on each platform

`Scripts/Tests/test-term-e2e.sh` drives `host_probe`'s `term.*` verbs against
a real `sirio-host` — one fresh data root per case, every host force-shut by
its case — and prints one `CASE <n> <name> PASS|SKIP` line per case, then
`TERM E2E OK`. It is the evidence for §6 of
`docs/superpowers/specs/2026-10-06-host-terminal-sessions-design.md`: the table
there is a hypothesis, and a row stays in it only while this probe (or the
reading it names) shows it working.

## Run it

    Scripts/Tests/test-term-e2e.sh --out-dir "$HOME/.cache/sirio-term-e2e"

`--out-dir` keeps each case's directory: the host logs and state files, the
attach outputs (`victim.out`, `stalled.out`, `attach.out`), the replicas and
host reads (`r.txt`, `h.txt`, `r2.txt`, `h2.txt`), the recorded offsets
(`off.txt`), and `transcript.log` with every probe call and its answer. Case 9
(conformance) is not repeated here — `test-host-e2e.sh` case 8 runs it.

## What each row proves

| Platform | Risk (spec §6) | Answer | Verified |
|---|---|---|---|
| Windows | A ConPTY child started by the host inherits the host's handles — SP1's `Command::spawn` lesson | `portable-pty-0.9.0/src/win/psuedocon.rs` spawns with `CreateProcessW(…, bInheritHandles = 0, …)`, `STARTF_USESTDHANDLES`, and `INVALID_HANDLE_VALUE` stdio, so a ConPTY child inherits nothing; the CI run of case 6 on Windows confirms the process side | Code read 2026-10-06 (below); process side: not yet run |
| Windows | ConPTY inside a `DETACHED_PROCESS` host with no console | Probe: cases 1–6 create, attach, read and close real sessions on exactly such a host | Not yet run — confirmed when `windows-check` runs cases 1–6 |
| Windows | Killing descendants is a no-op today | A job object per session with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; `term.close` closes it. Nested jobs (Windows 8+) make it work inside a `WindowsNoBreakaway` host too | Not yet run — case 6 on Windows |
| Windows | The "Running" pill is always false today | Stays false: the owner tick publishes `foreground = false` unconditionally on Windows (`#[cfg(windows)]` in `sirio_term_session/src/owner.rs`) — no `tcgetpgrp` equivalent is consulted and the ConPTY process list is not used | Code fact 2026-10-06; no probe planned |
| macOS | **TCC**: agents become children of a launchd job, not of Sirio.app; privacy prompts may name `sirio-host`, and permissions granted to Sirio may not apply | A probe with `sirio_privacy`'s `permissions` example run inside a host session, on the runner and on a real Mac (manual, Task 13) | Not yet run. Deferred by the user on 2026-10-12; owed before SP2 is called done on that platform |
| Linux, macOS | PTYs opened by a host that is a session leader | `O_NOCTTY` on every master and slave the host opens, plus `drop_acquired_controlling_terminal` giving up a PTY that became the host's terminal anyway; case 13 checks both sides | 2026-10-06, this machine (Linux): PASS; macOS, Windows: not yet run |
| Linux | The host's systemd scope also holds its sessions | Intended: stopping the scope stops everything in it | Not yet run |
| Windows | The app itself: a Windows Sirio adopts or starts the host, a terminal tab's session survives a relaunch, and Show session attaches without owning. `test-host-sessions-e2e.sh` runs on Linux and macOS only, so this is a manual run on a Windows desktop | Manual check, not in CI. Deferred by the user on 2026-10-12; owed before SP2 is called done on that platform | Not yet run |

## The Windows inheritance reading, 2026-10-06

`portable-pty` 0.9.0, `src/win/psuedocon.rs` (the spelling is upstream's),
`spawn_command`: the `STARTUPINFOEXW` carries `dwFlags =
STARTF_USESTDHANDLES` with `hStdInput = hStdOutput = hStdError =
INVALID_HANDLE_VALUE` — the comment there says explicitly that this keeps a
spawned process from inheriting redirected output handles when daemonizing —
and the `CreateProcessW` call passes `0` for `bInheritHandles` with
`EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT`, attaching the
ConPTY through a `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` instead. So a ConPTY
child starts with no inherited handles at all: SP1's `Command::spawn`
lesson (a host started with inherited stdout pipes keeping a reader waiting)
cannot recur through this spawn path. What the reading cannot show is the
process side — that closing a session really ends the whole tree — which is
why the row stays open until case 6 runs on Windows.

## The "Running" reason, 2026-10-06

On unix the owner tick reads the foreground process group off the master fd
(`tcgetpgrp`, compared against the session leader's pid) and publishes it as
`foreground.running`. On Windows there is no equivalent call in the tick —
`rust/crates/sirio_term_session/src/owner.rs` sets `let foreground = false`
under `#[cfg(windows)]` — so the pill stays false exactly as it does today.
If a later task finds a reliable source in the ConPTY process list, this row
and the pill change together; until then the row records the absence.

## Linux run, 2026-10-06

This machine, local run (no CI link yet):

    $ Scripts/Tests/test-term-e2e.sh --out-dir "$HOME/.cache/sirio-term-e2e"
    CASE 1 survives-client-kill PASS (n=10->20) 3s
    CASE 2 resume PASS 2s
    CASE 3 snapshot-fidelity PASS 2s
    CASE 4 query-one-reply PASS 2s
    CASE 5 lagged PASS (the session outran its stalled client) 20s
    CASE 6 close-kills-tree PASS 1s
    CASE 7 sirioctl-direct SKIP (Task 11)
    CASE 8 exited-then-idle PASS 2s
    CASE 10 flood PASS (lagged=0, measurement only) 60s
    CASE 11 resize-detached PASS 3s
    CASE 12 alt-screen PASS 5s
    CASE 13 no-ctty PASS 1s
    CASE 14 echo-latency PASS (p50=4ms p95=4ms max=4ms) 0s
    CASE 15 bounded-read PASS (524288 bytes in 0s) 1s
    TERM E2E OK (14 cases, 103s)

(Run twice green after fix round 1; the earlier 13-case runs are superseded.)

The 20 MB flood lagged the stalled client (`lagged=1` in `c5/stalled.out`)
without blocking the session, so the flood size stays as the brief wrote it —
per the controller ruling it is raised (e.g. to 60 MB) only if a finite flood
ends before the client lags. The 50 MB flood is read by a client attached
while it is flowing: it did not lag (`lagged=0`) and the replica agreed
byte-for-byte with the host's text. Echo latency is a measurement for the
record, never a bound.

## Found by this E2E: the snapshot did not restore the live cursor (fixed)

Case 10 first ran with the attach landing before the flood (an interim
weakening) because, as briefed, it failed intermittently: the replica ended up
exactly one row short with no lag or resync signalled. Diagnosis (byte-level
capture of a failing client's received stream, then deterministic in-process
repros needing no host at all) showed the snapshot dump
(`sirio_term_session::render_snapshot`, ghostty-vt `Formatter` with cursor and
tab stops) never restored the live cursor — after the content rows it emits SGR
reset, a `[24;1H` tab-stop anchor and HTS tab-stop setup, leaving the replica at
row 24 col 72 — and worse, it trimmed the trailing blank rows, so the live
cursor sat below the dump's last content row. When the first suffix bytes then
started with `\r`, the first suffix line overwrote the dump's last row instead
of starting a new one. Idle snapshots (cases 3 and 12) and suffixes starting
mid-row were unaffected, which is why only this case caught it.

Fix round 1 repaired `render_snapshot`: after the formatter output it recreates
the trimmed trailing blank run with line feeds, appends a CUP to the live
cursor, and — when `is_cursor_pending_wrap()` holds — moves to the last column
and re-emits that cell (content plus SGR style) so the pending state comes back
with it. Covered by two engine tests,
`a_snapshot_restores_the_live_cursor_for_the_bytes_after_it` (feed 1500
numbered lines, attach, feed `"\r"` + 1500 more → replica text equals the
host's) and `a_snapshot_preserves_a_pending_wrap` (an 80-column fed line sets
pending; the next printable must wrap), plus a unit test for the SGR encoder.
Case 10 is back to the brief's flood command, snapshotting mid-flow.

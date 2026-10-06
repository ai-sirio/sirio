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
(`off.txt`), and `transcript.log` with every probe call and its answer. Case
7 (`sirioctl-direct`) prints SKIP until Task 11 turns it on; case 9
(conformance) is not repeated here — `test-host-e2e.sh` case 8 runs it.

## What each row proves

| Platform | Risk (spec §6) | Answer | Verified |
|---|---|---|---|
| Windows | A ConPTY child started by the host inherits the host's handles — SP1's `Command::spawn` lesson | `portable-pty-0.9.0/src/win/psuedocon.rs` spawns with `CreateProcessW(…, bInheritHandles = 0, …)`, `STARTF_USESTDHANDLES`, and `INVALID_HANDLE_VALUE` stdio, so a ConPTY child inherits nothing; the CI run of case 6 on Windows confirms the process side | Code read 2026-10-06 (below); process side: not yet run |
| Windows | ConPTY inside a `DETACHED_PROCESS` host with no console | Probe: cases 1–6 create, attach, read and close real sessions on exactly such a host | Not yet run — confirmed when `windows-check` runs cases 1–6 |
| Windows | Killing descendants is a no-op today | A job object per session with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; `term.close` closes it. Nested jobs (Windows 8+) make it work inside a `WindowsNoBreakaway` host too | Not yet run — case 6 on Windows |
| Windows | The "Running" pill is always false today | Stays false: the owner tick publishes `foreground = false` unconditionally on Windows (`#[cfg(windows)]` in `sirio_term_session/src/owner.rs`) — no `tcgetpgrp` equivalent is consulted and the ConPTY process list is not used | Code fact 2026-10-06; no probe planned |
| macOS | **TCC**: agents become children of a launchd job, not of Sirio.app; privacy prompts may name `sirio-host`, and permissions granted to Sirio may not apply | A probe with `sirio_privacy`'s `permissions` example run inside a host session, on the runner and on a real Mac (manual, Task 13) | Not yet run |
| Linux, macOS | PTYs opened by a host that is a session leader | `O_NOCTTY` on every master and slave the host opens, plus `drop_acquired_controlling_terminal` giving up a PTY that became the host's terminal anyway; case 13 checks both sides | 2026-10-06, this machine (Linux): PASS; macOS, Windows: not yet run |
| Linux | The host's systemd scope also holds its sessions | Intended: stopping the scope stops everything in it | Not yet run |

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
    CASE 6 close-kills-tree PASS 0s
    CASE 7 sirioctl-direct SKIP (Task 11)
    CASE 8 exited-then-idle PASS 2s
    CASE 10 flood PASS (lagged=0, measurement only) 61s
    CASE 11 resize-detached PASS 3s
    CASE 12 alt-screen PASS 5s
    CASE 13 no-ctty PASS 0s
    CASE 14 echo-latency PASS (p50=4ms p95=4ms max=4ms) 0s
    TERM E2E OK (13 cases, 102s)

The 20 MB flood lagged the stalled client (`lagged=1` in `c5/stalled.out`)
without blocking the session, so the flood size stays as the brief wrote it —
per the controller ruling it is raised (e.g. to 60 MB) only if a finite flood
ends before the client lags. The 50 MB flood is read by a client attached
before it starts (the attach runs during a leading `sleep 5`; see the Ruling
below): it did not lag (`lagged=0`) and the replica agreed byte-for-byte with
the host's text. Echo latency is a measurement for the record, never a bound.

## Ruling: case 10 attaches before the flood (engine bug)

The brief's session floods at once, so the attach snapshots mid-flow. That
snapshot path has an engine bug outside this task, found while proving this
case: the snapshot dump (`sirio_term_session::render_snapshot`, ghostty-vt
`Formatter` with cursor and tab stops) does not restore the live cursor —
after the content rows it emits SGR reset, a `[24;1H` tab-stop anchor and HTS
tab-stop setup, leaving the replica at row 24 col 72 with no CUP to the live
cursor (the dump carries no save/restore/home for it). When the first suffix
bytes then start with `\r`, the first suffix line overwrites the dump's last
row instead of starting a new one, and the replica ends up exactly one row
short. No lag or resync is signalled; idle snapshots (cases 3 and 12) and
suffixes starting mid-row are unaffected, which is why only this case caught
it. Minimal in-process repro (attach needed no host at all):

```rust
// session running `sleep 30`; feed 1500 numbered lines, attach (snapshot),
// feed "\r" + 1500 more lines, apply snapshot + outputs to a fresh VT:
// the replica render misses exactly one fed line, deterministically.
```

The engine task owns the fix (e.g. emit the tab stops before the cursor
restore, or append a final CUP to the live cursor in `render_snapshot`).
Until then case 10 attaches during the leading sleep — the dump is trivial
and the whole flood arrives as outputs — and a mid-flow snapshot fidelity
case belongs back in this script once the dump is fixed.

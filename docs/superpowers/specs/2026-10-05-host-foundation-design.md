# A detached host behind the app — programme, and SP1: host foundation

**Date:** 2026-10-05
**Status:** proposed
**Programme:** SP1 of seven (§2). SP1 (this document) — the `sirio-host`
process, its lifecycle and its protocol, carrying nothing but `ping`, `info`
and one event topic. SP2–SP7 move terminals, chats, the agent runtime, the
workspace model and the execution operations into it, then open it to
remote clients. Built in order, each with its own spec.
**Prior art:** [unpeel](https://github.com/unpeel-com/unpeel) (HEAD
`2504ed8`) and [Orca](https://github.com/stablyai/orca) (HEAD `67fc708b`),
both read for this design; §3 records what was taken from each.

## §0 Intent

Today Sirio is one process: the gpui window owns every PTY (`TerminalView`
spawns it, `sirio_terminal/src/lib.rs`), every chat agent (`sirio_ui::chat`
holds the `ChatClient`), the activity model, the control socket and the
workspace state. When that process dies — a crash, a Force Quit, a Quit, an
update — every agent dies with it.

The user wants the split both reference projects converged on: a **host**
that owns everything that runs, and an **app** that is a client of it.
Three reasons, in the user's words of priority:

1. Agents survive **a crash or Force Quit of the app**, **a Quit and
   relaunch**, and **an update of Sirio**. A reboot of the machine is out of
   scope (no cold restore).
2. The host can later serve **remote or several clients** — a second
   desktop, a headless `sirio-host` on a remote machine.
3. **Architectural clean-up**: the host is the single source of truth, and
   `sirio`'s 42k-line `main.rs` shrinks toward a client.

Success for the programme: on macOS, Linux and Windows, an agent running in
a terminal or a chat tab keeps running through each event of (1), and the
reopened app shows it as it was. Success for SP1 (§9): a real `sirio-host`
is started, adopted, survives its client's death, idles out, and drains
across a protocol major — proven end to end.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Events agents must survive | App crash / Force Quit, Quit and relaunch, update of Sirio. **Not** a machine reboot. |
| What the host hosts | Terminals **and** chats (Claude native and ACP sessions). |
| Platforms | macOS, Linux **and Windows**, from the first sub-project. |
| Shape | **B — full runtime**: the host owns sessions *and* their semantics (activity, hooks, notifications policy, workspace model, git and filesystem operations); the app is a pure client. Not the thin session host (A), not unpeel's two-tier core + worker (C). |
| Why B | Remote / multiple clients later, and the host as single source of truth. |
| Decomposition | Seven sub-projects, SP1 → SP7 (§2), each with its own spec, plan and implementation. |
| When the host runs | **On demand locally** (started by the app, exits when idle); **always on only when explicitly installed** (`sirio-host install`, delivered in SP7). |
| A protocol major changes while sessions are live | **The app speaks majors N and N−1.** Old sessions stay usable on the old host through an adapter; new ones start on the new host; the old host exits once empty. Majors are rare by discipline (§6.4). |

## §2 The programme

| # | Sub-project | Delivers | Serves |
|---|---|---|---|
| **SP1** | **Host foundation** (this document) | `sirio-host` binary; detached start, discovery, liveness, versioned copy, drain, idle exit; transport-neutral router; framing, handshake, versioning, capability ledger, conformance fixtures; local transport with peer identity. Content: `host.ping`, `host.info`, `host.shutdown`, the `host.state` topic. | everything after it |
| SP2 | Terminal sessions | PTY, resident libghostty-vt grid and byte journal in the host; snapshot on attach; `TerminalView` becomes a client; the two PTY owners of today (`TerminalView`, `sirio_control::PaneRegistry`) become one. Quit becomes *detach*, with an explicit "end all sessions" action. | survival (1) |
| SP3 | Chat sessions | `ChatClient` in the host; journal of the in-flight turn's `AcpEvent`s (made serialisable) on top of the persisted transcript; `sirio_ui::chat` folds events received over the socket. | survival (1) |
| SP4 | Agent runtime | `AgentActivityModel`, hook ingestion, `sirioctl` against the host, notification *policy* in the host and *effect* in the app. | multiple clients |
| SP5 | Workspace model | Projects, worktrees, the session list per worktree; the `sirio_persistence` database moves to the host; the app keeps only layout (splits, focus, sizes). | single source of truth |
| SP6 | Execution operations | git, filesystem reads and writes, file watching, LSP, forge (`gh`/`glab` run where the repository lives). | prerequisite for remote |
| SP7 | Remote | `sirio-host install` (launchd / systemd `--user` / Task Scheduler), headless serve, SSH-stdio and/or TLS transport, pairing and per-device tokens, a second client. | remote clients |

Shrinking `main.rs` is not a sub-project: SP2–SP6 each carry their part of
it away. SP2 and SP3 come first after SP1 so that the survival criteria land
early; unpeel did it the other way round and lived for months with a
"compatibility host" inside its app.

## §3 What was taken from the prior art

| From | Taken | Left |
|---|---|---|
| unpeel | One transport-neutral router (`controller_api`) behind every transport; binary framing on one stream (`UPL1`) so SSH stdio is a transport like any other; `protocol/` ledger with capability minors and conformance cases; effects bound to a connection generation and never replayed; pid checked against start time before any signal. | Takeover by fd passing (`SCM_RIGHTS` — no Windows equivalent for ConPTY); the separate PTY-core process (SP1 has one host process); hand-written JSON as source of truth. |
| Orca | Drain instead of takeover — a host with live sessions is never replaced; endpoint namespaced by protocol version; the verdict vocabulary *loss of contact is never evidence of death*; running the host from a copy outside the installed package (`windows-daemon-host-relocation.md`); own systemd scope on Linux; `openEnum`-style degradation of unknown values. | 39 daemon protocol versions with adapters back to v1 (Sirio keeps one major back); typing the contract in a second language. |

## §4 Crates, processes, binary

### §4.1 Crates

| Crate | Role | Depends on |
|---|---|---|
| `sirio_ipc` (leaf) | The local transport, **extracted** from `sirio_control`: unix-socket listener and named-pipe listener (`windows_pipe.rs`), peer identity (`getpeereid` / `SO_PEERCRED` / `GetNamedPipeClientProcessId`), stale-endpoint probe-and-replace that never steals a live one, socket path length limits, and the process start-time read (`/proc`, `proc_pidinfo`, `GetProcessTimes`) the liveness verdict needs. `sirio_control` is rewired onto it with no change of behaviour. | — |
| `sirio_host_protocol` (leaf, no I/O) | Wire types (serde), frame codec, version negotiation, `Principal`, error codes, the liveness verdict as a pure function. | — |
| `sirio_host` (lib + bin `sirio-host`) | Router, listeners, lifecycle (lock, state file, idle exit, drain, shutdown). | `sirio_ipc`, `sirio_host_protocol` |
| `sirio_host_client` | Discover, adopt or start the host; connect; handshake; the N−1 adapter. Used by `sirio` now and by `sirioctl` in SP4. | `sirio_ipc`, `sirio_host_protocol` |

`sirio` depends on `sirio_host_client` and **never** on `sirio_host`. That
dependency edge is the boundary: the app cannot reach the host's internals,
only its protocol.

### §4.2 Processes

At most one live host per **protocol major** per **data root**. The data root
is `<data>/host/` under Sirio's per-user data directory, overridable with
`SIRIO_HOST_HOME` for tests and isolated runs (as `SIRIO_DB` is). The
endpoint carries the major — `host-v1.sock`, or
`\\.\pipe\sirio-host-v1-<user-sid-hash>` on Windows — so that during a drain
two majors coexist without contending for one endpoint, and neither host
needs to know about the other.

### §4.3 The binary and where it runs from

`sirio-host` ships beside `sirio` in all three packages:
`Sirio.app/Contents/MacOS/`, the AppImage's `usr/bin/`, the Inno install
directory. `Scripts/build-app-bundle.sh`, `Scripts/build-appimage.sh`,
`Scripts/build-inno.sh` and `build-release.yml` gain it; it is signed and
notarised with the app on macOS.

Before starting a host, the client **copies the packaged binary to
`<data>/host/bin/<version>/sirio-host`** (atomic: write to a temporary name,
then rename; skipped when the copy exists with the same size and SHA-256)
and starts it from there, on every platform:

- **Windows** needs it: Inno's Restart Manager closes processes holding files
  in the install directory, so a host started from there would be killed by
  the very update it must survive.
- **Linux AppImage** needs it: the packaged binary lives on a FUSE mount that
  disappears when the process that mounted it exits.
- **macOS** does not strictly need it (a replaced `.app` leaves the running
  inode alone) and gets it for uniformity.

A copy is deleted when no live host's state file names its version.

### §4.4 Modes

`on-demand` (the only mode SP1 starts): exits after a grace period with no
sessions and no clients (§5.5). `service`: never idles out — SP1 carries the
mode in `host.info` and the idle logic; installing a service is SP7.

## §5 Lifecycle

The rule this section exists to keep: **loss of contact is never evidence
that a process is dead.** Nothing in Sirio signals or replaces a host on the
strength of a failed connection alone.

### §5.1 State on disk

Per major, in the data root (directory mode `0700` on unix; on Windows the
directory inherits the user profile's ACL):

- the endpoint;
- `host-v<N>.lock` — an exclusive advisory lock (`flock` on unix,
  `LockFileEx` on Windows) **held for the host's whole life**, released by
  the kernel when the process dies;
- `host-v<N>.json` — `{pid, start_time, version, protocol: {major, minor},
  mode, endpoint, generation}`, written atomically after the endpoint is
  bound and before the first client is accepted.

### §5.2 Liveness verdict

A pure function of three observations — handshake outcome, lock state, and
whether a process with the recorded pid exists with the recorded start time
(`/proc/<pid>/stat` on Linux, `proc_pidinfo` on macOS,
`GetProcessTimes` on Windows):

| Verdict | When | What the client does |
|---|---|---|
| `Live` | the endpoint completes the handshake | adopt it |
| `Unverifiable` | no handshake, but the lock is held, or the pid exists with the same start time | wait and retry with backoff (up to 10 s), then report it; **never** start a second host, **never** signal |
| `Absent` | lock free, and the pid is gone or its start time differs (recycled pid) | clean the stale endpoint and state file, start a host |

### §5.3 Starting a host (`ensure_host`)

1. Discover: read `host-v<N>.json` for each major the client speaks (N,
   then N−1), compute the verdict.
2. `Live` → adopt. `Unverifiable` → §5.2. `Absent` → continue.
3. Copy the binary (§4.3) and start it detached (§5.4).
4. The new host takes the lock **before** binding. Two clients starting at
   once each spawn at most one candidate; the candidate that fails to take
   the lock exits immediately, and both clients wait for the endpoint of the
   winner.
5. Wait for the endpoint (bounded, 10 s), handshake, adopt.

### §5.4 Leaving the app's process group

The host must not belong to anything that dies with the app:

| Platform | Mechanism | Protects against |
|---|---|---|
| Linux | `systemd-run --user --scope --unit=sirio-host-v<N>-<rand>` when a user systemd instance answers; otherwise double fork and `setsid` | the desktop closing the app's `app-*.scope` (orca's `KillMode` finding) |
| macOS | a launchd job loaded on demand (`launchctl bootstrap gui/<uid>` of a plist generated under the data root, label `<bundle-id>.host.v<N>`, `RunAtLoad`, no `KeepAlive`) | Force Quit terminating the app's coalition (unpeel's finding, 2026-09-06) |
| Windows | `CreateProcessW` with `DETACHED_PROCESS \| CREATE_NEW_PROCESS_GROUP \| CREATE_BREAKAWAY_FROM_JOB \| CREATE_NO_WINDOW` from the copy under the data root | a job object with kill-on-close around the app |

**This table is a hypothesis until the probe confirms it.** The first task of
SP1's plan is a probe that starts a host each way and kills its parent each
way — SIGKILL, closing the scope, Force Quit, closing the job — and records
whether the host lived. A mechanism stays in this table only if the probe
shows it working; a failed row is redesigned before any other task starts.

On every platform the host's stdio is closed (or redirected to its log
file), its working directory is the data root, and it ignores `SIGHUP`.

### §5.5 Idle exit

In `on-demand` mode, after **60 s with zero sessions and zero connected
clients**, the host exits in this order: stop accepting, remove the
endpoint, remove its state file, release the lock (by exiting). A client
that connects inside the window resets it. A client that loses the race
sees `Absent` or `Unverifiable` and goes through §5.3 again.

### §5.6 Drain across a major

When the client finds a `Live` host whose major is **N−1** (it speaks N and
N−1):

- it keeps a connection to the N−1 host through the adapter, and its
  sessions remain usable there;
- it ensures a host of major N and starts **new** sessions there;
- the N−1 host exits by itself once it holds no sessions (§5.5).

When the client finds a host whose major it does **not** speak (newer than
N — a downgrade — or older than N−1), the handshake returns `Refused`, the
client does not connect, and says so (§8). In SP1 there are no sessions;
the drain is exercised through `SIRIO_HOST_PROTOCOL_MAJOR` (§9.2).

### §5.7 Stopping

`host.shutdown {force: bool}` refuses with `sessions_live` while sessions
exist unless `force` is true; `force` exists for an explicit user action
(SP2's "end all sessions") and for tests. Nothing else in Sirio ends a host:
the app never kills it, on Quit or otherwise.

**Quit semantics in SP1 are unchanged** — there are no sessions yet. The
`on_app_quit` → `PaneRegistry::shutdown` contract in `CLAUDE.md` changes in
SP2, where Quit becomes *detach*.

### §5.8 Logging

The host writes `<data>/host/log/host-v<N>.log`, rotated at 4 MiB with two
generations kept (orca records the absence of rotation as a pain point).
Lines carry no session content: the same rule as `sirio_perf` — names and
identifiers, never paths, prompts or scrollback.

## §6 Protocol

### §6.1 Framing

One connection carries everything, because a future transport (SSH stdio)
is a single byte stream. A frame:

```
[ "SRH1" 4B ][ kind u8 ][ flags u8 ][ reserved u16 = 0 ][ len u32 BE ][ payload: len bytes ]
```

| `kind` | Payload | Used from |
|---|---|---|
| `1` Request | JSON | SP1 |
| `2` Response | JSON | SP1 |
| `3` Event (host-initiated) | JSON | SP1 |
| `4` Data | raw bytes, with a stream-id header defined by SP2 | reserved in SP1, refused if received |

`flags` and `reserved` must be zero in major 1; a non-zero value is a
protocol error. Payload limit: **1 MiB** for kinds 1–3 (the limit
`sirio_control::MAX_BUFFER_BYTES` already enforces). A frame with a wrong
magic, an unknown kind, or an over-limit `len` gets an error response where
one is possible and the connection is closed — never buffered without bound.

`Data` is reserved now because adding a kind later would be a major change:
a peer of the current major would drop it silently — the failure Orca
records for un-negotiated opcodes.

### §6.2 Envelope and handshake

- Request: `{id: u64, method: string, params: object}`.
- Response: `{id, result: object}` or `{id, error: {code, message}}`. Codes
  are a closed enum on the host and an open one on the client
  (`#[serde(other)] Other`).
- Event: `{subscription: u64, seq: u64, payload: object}`; `seq` is
  monotonic per subscription, so a client that reconnects knows where it
  stopped.

The first frame on a connection is mandatory:

- client → `Hello {client_version, majors: [N, N−1], minor}`;
- host → `Welcome {host_version, protocol: {major, minor}, capabilities:
  [string], host_id, generation}` or `Refused {reason, host_major}`.

`generation` is fresh at every host start. Any request with an effect
carries the `generation` the client was welcomed with, and the host refuses
it with `stale_generation` if it differs — an effect is never replayed
against a host that did not see the original.

### §6.3 Router

```rust
Router::handle(&Principal, Request) -> Response
Router::subscribe(&Principal, topic) -> impl Stream<Item = Event>
```

`Principal` in SP1 is `{transport: Local, uid}`. The router never sees a
connection, a socket or a pipe; transports turn frames into calls and back.
This is what makes SP7 an addition and not a rewrite.

### §6.4 Compatibility rules

Written here as the contract every later sub-project inherits:

1. The major must equal one the client speaks; the minor is additive only.
2. Unknown fields are ignored; unknown enum values degrade to `Other` — the
   same posture `sirio_claude` takes toward Claude Code's protocol.
3. A new method or topic is enabled only through a capability in `Welcome`,
   never by probing a method and reading the error.
4. A change lands **host first**: the host gains the capability before any
   client relies on it.
5. A change that a peer of the same major could misread — a new frame kind,
   a field whose absence changes meaning, a narrowed type — is a **major**.
   A major is rare by discipline, and comes with the N−1 adapter in
   `sirio_host_client`, removed one stable release after.

### §6.5 Methods and topics in SP1

| Name | Kind | Capability | Effect |
|---|---|---|---|
| `host.ping` | method | `host.v1` | none |
| `host.info` | method | `host.v1` | none — `{version, pid, mode, sessions, clients, uptime_s, generation}` |
| `host.shutdown {force}` | method | `host.v1` | stops the host; `sessions_live` unless `force` |
| `host.state` | topic | `host.v1` | events `{clients, sessions, mode, draining}` on change |
| `host.debug.hold_session {held}` | method | none — **debug builds only**, absent from the ledger, answered `unknown_method` by a release build | counts a fake session, so tests can exercise `sessions_live` and drain before SP2 brings real sessions |

### §6.6 Source of truth and conformance

The Rust types in `sirio_host_protocol` are the source of truth. Committed
under `protocol/host-v1/`:

- `capabilities.json` — the ledger: every capability, the minor it arrived
  in, its methods and topics;
- `schema.json` — generated from the types with `schemars`;
- `conformance/*.json` — request / expected-response pairs.

A test regenerates the schema and fails when it differs from the committed
one: it guards the **wire contract**, so a change to it has to be committed
— and reviewed — on purpose. The conformance cases run against a real
`sirio-host` (§9.2). This removes the class of bug the unpeel survey found,
a ledger naming `/stream` where the code serves `/output`.

### §6.7 Authentication in SP1

Local only: the endpoint is `0600` inside the `0700` data root (on Windows,
the pipe's security descriptor grants only the current user), and the
peer's identity is checked against the host's own user before the
handshake, with `sirio_ipc`'s existing checks. A mismatched peer is closed
without a frame. Per-device tokens, TLS and pairing are SP7.

## §7 Integration in the app

At startup `sirio` calls `sirio_host_client::ensure_host()` on a background
executor — never on the GPUI thread — and keeps the connection.

**In SP1 nothing functional depends on the host.** If it cannot be reached
the app works exactly as today. Settings → General gains one diagnostic
row: host version, pid, mode, verdict, and the reason when it is not
`Live`. That row is SP1's only visible surface, and it is also how a person
checks the wiring by eye.

From SP2 on, the app fails closed when the host is unavailable ("Host not
available — Retry"), with no local fallback that would reintroduce a second
owner of PTYs. That is SP2's decision to make, recorded here so SP1 does not
pre-empt it.

## §8 Errors

| Situation | Behaviour |
|---|---|
| The host cannot be started (binary missing, copy failed, detached start failed) | The reason in the diagnostic row; no retry loop |
| `Unverifiable` | Backoff retries up to 10 s, then reported as such; never a second host |
| `Refused` — a major the client does not speak | Not connected; the diagnostic row names both majors (an older app facing a newer host after a downgrade) |
| Stale endpoint | Replaced only on `Absent` (`sirio_ipc`'s probe, behaviour already covered by `sirio_control`'s tests) |
| Peer of another user | Closed before the handshake |
| Frame errors (§6.1) | Error response when possible, connection closed |
| Effect with a stale `generation` | `stale_generation`; the client re-reads state, never re-sends |

## §9 Testing

### §9.1 Units, written failure-first

Only the pure units of `sirio_host_protocol`, and for each, every way it can
fail is written before the code:

- **frame codec** — wrong magic, truncated header, truncated payload,
  over-limit `len`, unknown kind, non-zero `flags`/`reserved`, two frames in
  one read, one frame across reads;
- **version negotiation** — host N with client [N, N−1]; host N−1; host
  N+1; host N−2; minors on either side;
- **liveness verdict** — every combination of handshake ok/failed, lock
  held/free, pid absent/present-same-start/present-different-start.

### §9.2 End to end — `Scripts/Tests/test-host-e2e.sh` → `HOST E2E OK`

Against real binaries, with `--out-dir DIR` keeping the transcript and every
host's log and state files, and `SIRIO_HOST_E2E_VERBOSE=1` printing each
stage:

1. A client starts the host; a second client adopts it — one pid.
2. Two clients start at once — one host.
3. SIGKILL of the client that started it — the host lives; on Linux, also
   stopping the client's systemd scope.
4. Idle exit after the grace period (shortened by
   `SIRIO_HOST_IDLE_GRACE_MS`, debug builds only) — endpoint, state file and
   lock gone.
5. A planted state file with a recycled pid (wrong start time) — `Absent`,
   and no signal sent to that pid.
6. A live endpoint is never stolen by a second host.
7. Version skew through `SIRIO_HOST_PROTOCOL_MAJOR` (debug builds only, as
   `SIRIO_FORGE_TEST_ENDPOINTS` is): an N−1 host and an N host coexist, the
   client talks to both, the N−1 host exits once empty; a host of a major the
   client does not speak is `Refused`.
8. Every case in `protocol/host-v1/conformance/` against the real host.
9. `host.shutdown` without `force` while a fake session is held
   (`host.debug.hold_session`) — `sessions_live`; and an N−1 host holding
   one does not exit until it is released.

### §9.3 Platforms

- Linux: the E2E runs locally and in `pr.yml`.
- macOS: the E2E is added to `macos-check.yml` (`macos-15`).
- Windows: today Windows builds only in the release job. SP1 adds a
  `windows-check.yml` on `windows-latest` for pull requests that builds the
  host crates and runs the E2E.
- **Force Quit on macOS cannot be simulated on a runner** (it is the real
  Force Quit window). It is part of the probe (§5.4), done by hand, with its
  artefact — a recording and the host's log — kept under `docs/testing/`.
  This is stated, not glossed over.

## §10 Out of scope for SP1

Sessions of any kind; changing Quit; `sirio-host install`; any transport but
the local one; tokens, TLS, pairing; moving any state out of `sirio`;
`sirioctl` against the host; a UI beyond the diagnostic row.

## §11 Open points carried to later sub-projects

- **SP2** decides the `Data` frame's stream-id header, the journal format,
  and whether the PTY reactor is one thread for all sessions (unpeel's
  core) or per session.
- **SP2** decides the "end all sessions" action and its confirmation.
- **SP3** decides whether `AcpEvent` itself becomes the wire type or a
  versioned mirror of it (the former ties `sirio_acp`'s internals to the
  protocol's compatibility rules).
- **SP4** decides whether the existing control socket (`control.sock`) is
  served by the host or folded into the host protocol.
- **SP7** decides SSH stdio vs TLS vs both, and the per-principal model
  (unpeel's devices are all owner-equivalent today).

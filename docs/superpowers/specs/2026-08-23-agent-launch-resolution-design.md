# Agent launch resolution — Design

Date: 2026-08-23
Branch: `worktree/silver-forest-e51f`

## Goal

Make "how does Tiller launch agent X for a chat tab" a resolved fact rather than a
compile-time assertion, so that all five built-in agents can hold a chat, none of
them needs a package downloaded from the network at launch, and the ACP registry's
catalogue becomes installable and updatable from Settings.

## Non-goals

- **The three native protocol drivers** (Claude `stream-json`, Codex `app-server`,
  Pi `--mode rpc`). They are ~2,300 lines of retired Swift and get their own spec.
  This design leaves the seam where they plug in and does nothing else about them.
- **`uvx` distribution.** Two of the registry's 39 agents use it. The model decodes
  it; the installer rejects it with a named error rather than a silent `None`.
- **Chat transcript, persistence and rendering.** `tiller_acp::chat` and
  `tiller_ui::chat` keep their current contracts; only the `AgentCommand` handed to
  `Chat::launch_with_command` changes provenance.

## What is broken today

Three defects, one root cause.

**1. Two adapters deny a capability their CLI has.** `opencode.rs:87` returns `None`
under the comment *"OpenCode is a terminal-first CLI with no ACP server"*, and
`omp.rs:112` returns `None` under *"Oh-My-Pi is a pi fork running as a TUI; it has
no ACP server"*. Both are false. Verified live on 2026-08-23 against the installed
binaries — an ACP `initialize` request piped into `opencode acp` answers:

```
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,
 "agentCapabilities":{"loadSession":true,
 "sessionCapabilities":{"close":{},"fork":{},"list":{},"resume":{}}},
 "agentInfo":{"name":"OpenCode","version":"1.18.21"}}}
```

The retired Swift app launched Oh-My-Pi the same way — `AgentLaunchSpec.swift:73-75`
at commit `5430d7bf` builds `exec omp acp`. The consequence users see is the pill in
Settings: `acp_status_label` (`tiller_agents/src/lib.rs:60-66`) renders
*"No ACP server"* for two agents that serve one.

**2. The two agents that do connect fetch a package on every launch.**
`claude.rs:136` and `codex.rs:60` both spawn `npx -y <package>@latest`. That is a
network round-trip before a chat can start, against an unpinned version. The same
literal is duplicated across crates. Measured on 2026-08-23: **11 occurrences of
`claude-agent-acp` in 8 files** across `tiller`, `tiller_acp`, `tiller_agents` and
`tiller_ui`, and 6 of `codex-acp` in 5 files — including two independent hardcoded
defaults in `tiller_ui/src/chat.rs:1029-1030` and `:1090-1091`.

(The FABLE-12 audit cited below counted 8 in 7 files. Re-measuring rather than
quoting it is the point of this whole spec: the literal spread while the number
sat still in a document.)

**3. The root cause.** `AgentAdapter::acp_program` (`tiller_agents/src/lib.rs:181`)
returns `Option<AcpProgram>`, and `AcpProgram` (`:102`) holds `&'static str` plus
`&'static [&'static str]`. It is a claim about a third-party binary, frozen at
compile time, about a program that ships new versions independently. Defect 1 is
not a wrong value — it is what this shape produces over time. `Refresh` in Settings
(`settings.rs:1869`) re-runs `try_discover_availability`, which only looks up PATH;
nothing ever re-checks the capability claim.

This is already on the record. `docs/linux-rewrite/INVENTORY-AUDIT.md:77-97`
(FABLE-12) states that the Swift `TillerACP` package — 7,016 lines of Sources
(`:81`) — never entered the port inventory, and names this exact regression: the
Swift `AgentInstaller` installed `agent.id@agent.version` **once** (`:93`) and then
ran the local binary from `node_modules/.bin`, where the Rust port does
`npx -y ...@latest`, *"non pinnato, rete a ogni lancio dell'agente"* (`:97`).

## Ground truth

Everything below was verified on 2026-08-23, not taken from documentation.

| Fact | Evidence |
|---|---|
| ACP registry is live | `GET https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` returns `200`, 50,610 bytes, `version: "1.0.0"`, 39 agents |
| Distribution kinds | `npx` 21, `binary` 18, `uvx` 2 |
| Binary platform coverage | `linux-x86_64` 18, `linux-aarch64` 16, `darwin-aarch64` 18, `darwin-x86_64` 16, `windows-x86_64` 18, `windows-aarch64` 9 |
| Registry pins versions | `claude-acp` to `@agentclientprotocol/claude-agent-acp@0.70.0`; `codex-acp` to `@agentclientprotocol/codex-acp@1.6.2`; both match the current npm `latest` |
| An ACP wrapper for Pi exists | `pi-acp` to `npx pi-acp@0.0.33` |
| The registry publishes OpenCode's ACP invocation | `opencode` uses `binary`, per platform: `{ archive, cmd: "./opencode", args: ["acp"], sha256 }` — hash present on every platform |
| Artifact formats across all `binary` agents | 95 artifacts: `.tar.gz` 55, `.zip` 32, `.tar.bz2` 4, bare executable 4 |
| Integrity coverage | `sha256` on 48 of 95 artifacts; all 95 URLs are `https`; all 21 `npx` packages are version-pinned |
| Installed CLI versions on the dev machine | claude 2.1.241, codex-cli 0.149.0, opencode 1.18.21, pi 0.84.2, omp absent |
| Codex has no `acp` subcommand | `codex --help` lists `mcp-server` and `app-server`, not `acp` |

## Architecture

A new leaf crate **`tiller_registry`** (deps: `serde`, `serde_json`, `anyhow`,
`ureq`). It depends on no other Tiller crate, and — deliberately — **`tiller_agents`
does not depend on it**, so the "leaves — no local deps" contract in CLAUDE.md
holds for both.

```
tiller_registry (new leaf)        tiller_agents (leaf, unchanged deps)
        ^                                  ^
        +---------- tiller_ui -------------+
        +---------- tiller (main.rs) ------+
```

They meet in a pure function owned by `tiller_registry`:

```rust
pub fn resolve(input: ResolveInput) -> LaunchSource

pub struct ResolveInput<'a> {
    pub adapter_id: &'a str,
    /// The adapter's in-binary claim, AND whether that binary is on PATH right
    /// now. A claim alone is not enough — see below.
    pub builtin: Option<BuiltinAcp>,
    pub builtin_on_path: bool,
    /// Whether the manifest's recorded executable still exists on disk.
    pub installed: Option<(InstalledAgent, bool /* executable_exists */)>,
    pub registry: Option<&'a AcpRegistry>,
    pub platform_key: &'a str,          // "linux-x86_64", ...
}
```

No I/O, no clock, no process: the caller performs the two existence checks and
passes the answers in. This matters — a first draft of this design had `resolve`
take only the static claim, which meant an uninstalled `opencode` still resolved to
`Builtin` and shadowed a perfectly good managed copy, failing at `exec` with no way
down the ladder. `tiller_agents::availability()` (`lib.rs:142`) already computes the
PATH lookup, so the input costs nothing new.

`tiller_agents` passes its own honest claim; `tiller_registry` layers what it knows
from disk and network on top. Neither crate learns about the other.

### Who owns the blocking work

**`tiller` (`main.rs`), not `tiller_ui`.** `ureq` fetches, `npm install` and archive
extraction are long operations, and `tiller_ui` is the UI-primitives crate — giving
it TLS, process spawning and filesystem writes would be a boundary regression, not a
convenience. `tiller` owns `RegistryClient` and `Installer`, runs them on GPUI's
background executor, and hands `tiller_ui` finished state (`LaunchSource`, version,
in-flight/failed flags) to render. `tiller_ui` gets `tiller_registry` only for its
types.

### Documentation that must move with the code

`CLAUDE.md`'s crate-boundary diagram enumerates the crates and their edges. A new
crate makes it wrong on the day it lands, so it is updated in the same change — not
as follow-up.

### Why a new crate rather than `tiller_acp`

`tiller_acp` is the protocol crate: JSON-RPC framing, `AcpClient`, session events.
Package management is a different concern with a different failure surface, and
folding it in would force a new `tiller_agents -> tiller_acp` edge that does not
exist today. The rejected alternative is recorded here because it is cheaper and
would have worked; the boundary is the reason to spend the extra crate.

## Components

### `model.rs` — the registry payload

```rust
pub struct AcpRegistry { pub version: String, pub agents: Vec<RegistryAgent> }

pub struct RegistryAgent {
    pub id: String, pub name: String, pub version: String,
    pub description: Option<String>, pub repository: Option<String>,
    pub website: Option<String>, pub authors: Vec<String>,
    pub license: Option<String>, pub icon: Option<String>,
    pub distribution: Distribution,
}

pub enum Distribution {
    Npx { package: String, args: Vec<String> },
    Binary(BTreeMap<String, BinaryArtifact>),   // key: "linux-x86_64", ...
    Uvx { package: String, args: Vec<String> },
    Unknown,
}

pub struct BinaryArtifact {
    pub archive: String, pub cmd: String,
    pub args: Vec<String>, pub sha256: String,
}
```

`Distribution::Unknown` is required, not defensive: the registry changes without
Tiller releasing anything, so a distribution kind added in a registry minor must
not fail the whole decode. This is the same rule as `SessionUpdate.unknown` in the
Electron transport spec, and it is the direct antidote to defect 3 — the payload is
allowed to know things this build does not.

### `client.rs` — fetch, validate, cache

```rust
pub struct RegistryClient { /* cache_path, fetch, now — all injected */ }

impl RegistryClient {
    pub fn registry(&self, max_age: Duration, force: bool) -> Result<AcpRegistry>;
}
```

Rules, ported from `AgentRegistryClient.swift:8-9,33-45`:

- Cache younger than `max_age` (default 24h) is served without touching the network.
- A failed fetch falls back to any cached copy.
- The payload is decoded and validated **before** the cache is overwritten: a bad
  payload never clobbers a good cache.
- The cache is written **temp + `rename`**. It is read on every Settings → Agents
  open, so a process killed mid-write would otherwise leave a truncated file that
  fails to decode from then on — the same atomicity the installer gets, applied to
  the more frequently read path.
- A cache file that fails to decode **on read** (truncated by an older build, edited
  by hand, disk corruption) is treated as absent, not as an error: fetch, and if
  that fails too, report no registry rather than refusing to start.

`fetch` and `now` are constructor-injected, so every rule above is a unit test with
no network and no wall clock.

### `store.rs` — what is installed

A JSON manifest under the XDG data directory (`$XDG_DATA_HOME/tiller/agents/`). The
root is injectable; tests use a temp dir.

`tiller_control` already resolves an XDG-ish path for its socket, including the
Windows branch in `windows_pipe.rs` where no `XDG_*`/`HOME` exists. Copying that
logic into a second crate would be this spec's own defect 2 in miniature — a literal
spreading while nobody watches. But `tiller_registry` must not depend on
`tiller_control` (that crate depends on `tiller_acp` and `tiller_persistence`;
the edge would drag both into a leaf). **Decision:** hoist the directory resolution
into a tiny shared helper crate rather than duplicate it, and if that proves heavier
than it looks during implementation, duplicate it with a comment in each copy naming
the other. What is not acceptable is duplicating it silently.

```rust
pub struct InstalledAgent {
    pub id: String, pub version: String,
    pub executable: PathBuf, pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl InstallStore {
    pub fn manifest(&self, id: &str) -> Option<InstalledAgent>;
}
```

### `installer.rs` — install `id@version` once

Staging directory, then verify, then an atomic-enough swap, then write the manifest,
following `AgentInstaller.swift:95-135`.

- **`Npx`**: `npm install --prefix <staging> <package>`, then pick the executable in
  `node_modules/.bin` with the `resolveBinName` rule (`AgentInstaller.swift:139-156`):
  a single entry wins; otherwise the bin matching the package's own name (scope and
  version stripped) wins; otherwise the longest entry contained in the package name.
  **This rule is load-bearing.** Installing `codex-acp` also drops a `codex` bin from
  its `@openai/codex` dependency, and preferring it would silently launch the wrong
  program. The Swift comment says it outright: *"must never be preferred."*
- **`Binary`**: download the artifact for the current platform key, **verify its
  `sha256` when the registry publishes one**, unpack, check `cmd` exists, set the
  executable bit. "Unpack" is four paths, not one — measured across the 95
  artifacts: `.tar.gz` (55), `.zip` (32), `.tar.bz2` (4, `goose`), and **4 that are
  not archives at all** but bare executables (`sigit-linux-amd64`,
  `sigit-win-amd64.exe` and their siblings). The bare-executable case is not an
  edge case to bolt on later: it is the path with no extraction step, so treating
  every artifact as an archive would fail on it outright.
- **`Uvx`**: `Err(InstallError::UnsupportedDistribution)`, naming the agent.

Two deliberate departures from the Swift original:

1. **sha256 is verified.** Swift downloaded and extracted without checking. The
   registry now publishes the hash per archive, and this code downloads executables
   from the internet and runs them; verification is not optional.
2. **Extraction happens in-process.** Swift shelled out to `ditto -x -k`, `tar -xzf`
   and `chmod +x`. `ditto` does not exist on Linux, and none of the three is
   guaranteed on Windows. Use Rust crates for zip/tar and set the mode via
   `std::os::unix::fs::PermissionsExt` under `cfg(unix)`. This is a property that
   lived in the macOS environment and would otherwise vanish in the port without
   leaving a diff — failing only on the user's machine, at install time.

#### The parts that are easy to get wrong

- **The swap is not atomic, and the spec should not pretend otherwise.** `rename`
  onto a non-empty directory fails on Linux, so the sequence is necessarily
  remove-old-then-move-new, and a crash inside that window leaves *nothing* — which
  contradicts a naive "an interrupted install leaves the previous version". Install
  into `<root>/<id>/<version>/` and have the manifest name the version directory:
  the swap becomes a manifest rewrite (single file, temp + `rename`, genuinely
  atomic), the old version stays on disk until a later sweep removes it, and a crash
  at any point leaves the previously-recorded version fully intact.
- **Staging is named and collected.** `<root>/.staging/<id>-<version>-<pid>`; a
  sweep at startup removes staging directories not owned by a live process, and a
  per-agent lock file makes a double-click on Install a no-op rather than two
  concurrent extractions into the same path.
- **Timeouts and size caps.** Connect and read timeouts on every `ureq` call, a
  ceiling on archive download size, and a ceiling on *extracted* bytes — a
  decompression bomb fills the disk long before a stream-computed sha256 can reject
  it. There is no "in-flight forever" state.
- **`archive` URLs must be `https`.** Validated at decode. As published, the field is
  free text; `http://` (downgrade) or `file://` must be rejected before any fetch.
- **Extraction rejects more than `..`.** The dangerous entries are not only
  traversal paths: a tar entry may be a symlink or hardlink pointing outside the
  destination, which passes a naive `..`/absolute check. The rule is: reject any
  non-regular, non-directory entry, and reject any path that does not resolve inside
  the destination.
- **The executable bit is set on `cmd` only.** `tar` preserves modes and `zip` does
  not, so the two formats would otherwise behave differently; setting exactly one
  known path makes them behave the same and grants nothing extra.
- **`npm` has more failure modes than "absent".** `EACCES` on the prefix, a
  non-zero exit, an unreachable npm registry — each surfaces the captured output,
  because a bare "install failed" on a package manager is unactionable.

## Identity: adapter ids are not registry ids

The two namespaces do not match, and nothing joins them by accident:

| Adapter id (`tiller_agents`) | Registry id | Note |
|---|---|---|
| `claude` | `claude-acp` | the wrapper is a different package from the CLI |
| `codex` | `codex-acp` | same |
| `pi` | `pi-acp` | same |
| `opencode` | `opencode` | ids coincide; the entry is a `binary` distribution of the CLI itself |
| `omp` | *(absent)* | not in the registry; `Builtin` only |

So the join is an **explicit table**, `registry_id(adapter_id) -> Option<&str>`,
living in `tiller_registry` next to `resolve`. It is not name matching and not a
substring heuristic: `claude` is a prefix of nothing useful, and guessing here would
reintroduce exactly the class of silent-wrong-program bug that `resolveBinName`
exists to prevent. The retired Swift app had the same table under a different name
(`AgentIdMigration.canonical`, consulted at `AgentDriverFactory.swift:12`).

The remaining 35 registry agents have no adapter and are addressed by their registry
id directly.

## The resolution ladder

```rust
pub enum LaunchSource {
    Builtin { program: String, args: Vec<String> },       // in-binary ACP subcommand
    Installed(InstalledAgent),                            // Tiller-managed, pinned
    Installable { agent: RegistryAgent },                 // in registry, not installed
    Unavailable,
}
```

Order: **`Builtin` > `Installed` > `Installable` > `Unavailable`.** The contested
step is the first one.

Each rung is conditional on the thing actually being there, which is why `resolve`
takes existence answers rather than claims:

| `builtin` claim | on PATH | manifest | exe exists | registry entry | platform artifact | → |
|---|---|---|---|---|---|---|
| `Some` | yes | — | — | — | — | `Builtin` |
| `Some` | **no** | yes | yes | — | — | `Installed` |
| `Some` | **no** | — | — | yes | yes | `Installable` |
| `None` | — | yes | yes | — | — | `Installed` |
| `None` | — | yes | **no** | yes | yes | `Installable` (manifest is stale; reinstall) |
| `None` | — | — | — | yes | **no** | `Unavailable { reason: NoArtifactForPlatform }` |
| `None` | — | — | — | yes (`Unknown` dist) | — | `Unavailable { reason: UnsupportedDistribution }` |
| `None` | — | — | — | no | — | `Unavailable { reason: NotInRegistry }` |

Two rows deserve their names. **A stale manifest is not an error** — an agent whose
directory the user deleted, or whose `node_modules` was pruned, falls back to
`Installable` and reinstalls on one click, rather than failing at `exec`. And
**missing platform artifact is a real case, not a hypothetical**: the ground-truth
table above shows `linux-aarch64` at 16 artifacts against `linux-x86_64`'s 18, so at
least two registry agents cannot be installed on ARM Linux at all. `Unavailable`
carries a reason precisely so the row can say which of these it is instead of
rendering an undifferentiated grey pill — the failure mode this whole spec exists to
remove.

If `opencode` is on PATH and speaks `acp`, Tiller uses it and downloads nothing —
even though the registry offers the same 1.18.21 as an archive. The reasons are
concrete: the user installed and updates that CLI; a second copy would cost hundreds
of megabytes; and, decisively, the terminal pane already launches the user's copy
(`opencode.rs:73-74`), so preferring a managed copy would put two different versions
of the same agent in one window.

`Builtin` is claimed by `tiller_agents` and stays a static, honest, in-binary-only
statement — the shape it is actually good at:

| Adapter | `builtin_acp` | Why |
|---|---|---|
| `opencode` | `Some(("opencode", ["acp"]))` | verified handshake, 1.18.21 |
| `omp` | `Some(("omp", ["acp"]))` | `AgentLaunchSpec.swift:73-75`; **not yet verified live** — omp is not installed on the dev machine |
| `claude` | `None` | ACP lives in a separate npm package |
| `codex` | `None` | `codex --help` has no `acp` subcommand |
| `pi` | `None` | ACP lives in `pi-acp`; the native path is `--mode rpc` |

Everything that is *not* an in-binary subcommand moves out of the adapters and into
registry data. The `npx ...@latest` literals in `claude.rs`, `codex.rs` and
`tiller_ui/src/chat.rs:1030` are deleted, not relocated.

## Update policy

Because `Builtin` wins, Tiller does not own the update of a local CLI — `opencode`,
`claude` and `pi` are updated by the user's package manager, and stepping in would
be wrong. Automatic updating therefore applies only to **Tiller-managed** agents.
For local ones the honest maximum is to *inform*.

| Row kind | Source | Tiller may update? |
|---|---|---|
| `opencode`, `omp` (local binary, in-binary ACP) | user | no — show the version, and that a newer one exists |
| `claude-acp`, `codex-acp`, `pi-acp` (pinned package) | Tiller | yes |
| the other 35 registry agents, once installed | Tiller | yes |

(39 registry agents, minus the three pinned rows named above, minus `opencode`,
which the registry lists but which resolves as `Builtin` when the CLI is present.)

**Cadence:** the registry is re-checked when Settings → Agents opens and when
Refresh is pressed, subject to the 24h cache — roughly one request per day.
Installing an update takes one click; it never happens silently. A silent update
changes which code runs on the user's machine, and an agent broken upstream would
become a chat broken for no visible reason. With a click there is always a culprit
and a moment.

## Error handling

Extending the repo's existing rule that *"`None` is an honest answer"* to every way
this can fail:

- **No network.** Cache is served under a discreet banner. An already-installed
  agent still launches — that is the whole point of installing instead of `npx`.
- **Invalid payload.** The good cache is untouched; the error renders over the
  previous rows, exactly as `apply_agent_discovery` (`settings.rs:1893`) already
  keeps the last successful sweep visible under its banner.
- **sha256 mismatch.** Install aborts, staging is removed, the error is explicit.
  There is no "try anyway" fallback.
- **Install fails midway.** Staging is a separate directory and the previous `final`
  stays in place until the swap. An interrupted install leaves the previous version,
  not a broken one.
- **`npm` missing.** An error naming the remedy — `npx` distribution needs Node, and
  saying so beats a silent `None`.

## UI changes

The Agents screen changes little but substantially: the
*"ACP chat available" / "No ACP server"* pill stops being a compiled assertion and
becomes a rendering of `LaunchSource`. For OpenCode and Oh-My-Pi it becomes true.
Each row gains its version and, where meaningful, an action (`Install` / `Update`)
with in-flight and failed states.

This also closes `F-SET-18`, left *half-proven* in
`docs/linux-rewrite/triage/T3-set.md:20` precisely because *"no Install/progress/
Update/Retry control drawn"*.

## Testing

The whole core is testable with no network, no shared disk and no processes.

- **`resolve`** — truth table: builtin plus installed, builtin only, installed only,
  registry only, nothing, unknown distribution.
- **`client`** — injected `fetch`/`now`: fresh cache, stale cache, failed fetch with
  cache present, corrupt payload that must not overwrite.
- **`model`** — a committed fixture of the real `registry.json` (39 agents), plus a
  fixture carrying an invented `distribution` kind that must decode as `Unknown`.
- **`registry_id`** — the adapter-to-registry join, including `omp` mapping to
  nothing and `opencode` mapping to itself.
- **`installer`** — `resolveBinName` against the `codex` / `codex-acp` trap;
  matching and mismatching sha256; the manifest rewrite as the commit point, with a
  simulated crash before it leaving the previous version resolvable; a staging
  directory surviving a killed install and being swept at startup.
- **extraction safety** — fixtures for a `..` traversal entry, an absolute-path
  entry, a symlink pointing outside the destination, and an archive whose extracted
  size exceeds the cap. Each must be refused, not sanitised-and-accepted.
- **URL validation** — an `archive` field carrying `http://` or `file://` is
  rejected at decode, before any request is made.
- **Conformance (gated)** — launches the real `opencode acp` and `omp acp` and
  asserts the `initialize` handshake, SKIPping when the binary is absent, in the
  style `Scripts/ci-linux.sh` already uses for stages allowed to SKIP. This is the
  test that would have caught defect 1.

`Scripts/ci.sh` must print `CI OK` before this is considered done.

## Security model

This subsystem downloads executables from a third-party document and runs them. The
model has to be stated, not implied.

### What the sha256 verification does *not* cover

The "sha256 is verified" departure above reads like a strong guarantee. It is not a
general one, and the asymmetry must be on the record:

| Distribution | Agents | Integrity check | Code execution at install |
|---|---|---|---|
| `binary` | 18 | sha256 **on 48 of 95 artifacts** — 9 of the 18 agents publish at least one artifact with no hash at all | none — download, verify, extract |
| `npx` | **21** | **none** | **`npm install` runs the package's `preinstall`/`postinstall` scripts with full user privileges** |
| `uvx` | 2 | n/a — rejected | n/a |

So the majority distribution has no integrity verification at all, and installing
one runs maintainer-controlled code *before* the user has ever chosen to launch that
agent. The registry pins a version, which prevents drift; it does not verify that
the npm tarball matches anything the registry says.

**And the `binary` row is weaker than it first reads.** Measured on 2026-08-23:
`sha256` is present on 48 of 95 artifacts; the agents publishing at least one
unhashed artifact are `antigravity-acp`, `cortex-code`, `corust-agent`, `crow-cli`,
`cursor`, `devin`, `junie`, `stakpak` and `vtcode`. An earlier draft of this spec
asserted sha256 covered all 18, which was wrong.

**Decision for unhashed artifacts:** install is still offered, but the row says
*"no published checksum"* before the click, and the manifest records
`integrity: none` so what was installed unverified stays auditable afterwards.
Refusing outright would make half the binary catalogue uninstallable while the
user's fallback — downloading the same file by hand — carries strictly less
ceremony, so a refusal would buy no safety. `opencode`, the agent this spec
actually turns on, publishes a hash on every platform.

This is not fatal — every one of these agents will, once launched, execute arbitrary
code as the user by design; that is what a coding agent is. But there is a real
difference between "code runs because I started the agent" and "code ran because I
clicked Install", and the second deserves an explicit decision rather than a silent
inheritance from the Swift original.

**Mitigation adopted:** `npm install --ignore-scripts` for the `npx` path, falling
back to a second attempt *with* scripts only after telling the user which package
asked for it and why. This keeps the common case script-free without breaking a
package that genuinely needs a build step.

### Trust root

The sha256 lives in the same document as the URL it protects, fetched over one TLS
connection from one CDN. That is integrity against corruption and transit tampering,
**not provenance**: whoever controls `registry.json` controls what every Tiller user
installs on their next click of Update. There is no signature to check today —
the registry publishes none.

Accepted as a residual risk, with the boundaries that make it bounded: installs only
ever happen on an explicit click (never silently — see Update policy), the resulting
version is recorded in the manifest, and `Builtin` precedence means the five agents
users actually run are, in the common case, their own local CLIs rather than
anything this subsystem downloaded.

## Risks and open points

- **`omp acp` is unverified on this machine.** It rests on the Swift source
  (`AgentLaunchSpec.swift:73-75`) and on omp being a pi fork. This is, uncomfortably,
  the same shape as the defect this spec exists to fix: a static claim about a
  third-party binary that nobody re-checked. It is therefore a **ship gate, not a
  note** — `OhMyPiAdapter::builtin_acp` stays `None` until a real `omp` answers an
  `initialize` handshake. The conformance test SKIPs when the binary is absent, so
  an unverified claim can never turn into a false green either way.
- **`ureq` plus TLS is a new dependency class for this workspace**, which today
  carries only `gpui`, `anyhow`, `serde`, `serde_json` and `smallvec`. Blocking by
  design, owned by `tiller`, run on the background executor.
- **Pi is about to be reachable two ways.** This spec makes `pi-acp` installable
  while the deferred driver spec would give Pi a native `--mode rpc` driver. They are
  not in conflict — the ladder gains a rung above `Builtin` when that lands — but
  whichever ships second must not silently duplicate the other's row in Settings.
- **What a chat tab shows for `Unavailable`, or when a handshake fails at launch,**
  is named by the reason enum but not designed here; it belongs with the Settings
  row work.

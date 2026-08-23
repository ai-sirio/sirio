# Agent Launch Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the compile-time claim about which agents speak ACP with a resolved fact, so all five built-in agents can hold a chat and none needs a package fetched from the network at launch.

**Architecture:** A new leaf crate `tiller_registry` owns the ACP registry payload, a disk cache, an install store and an installer. A pure `resolve()` function ranks four launch sources (`Builtin` > `Installed` > `Installable` > `Unavailable`) from existence facts its caller supplies. `tiller_agents` keeps only its in-binary claim and never learns about the registry; `tiller` (`main.rs`) owns every blocking operation and hands `tiller_ui` finished state.

**Tech Stack:** Rust 2024, `serde`/`serde_json`, `ureq` 3 (blocking HTTP + rustls), `sha2`, `zip`, `tar` + `flate2`, standard `#[test]`, GPUI background executor for orchestration.

**Spec:** `docs/superpowers/specs/2026-08-23-agent-launch-resolution-design.md` (commits `2f31769a`, `1168f670`)

## Global Constraints

- `tiller_registry` is a **leaf**: it must not depend on any other `tiller_*` crate. `tiller_agents` must not depend on `tiller_registry`. Verify with `cargo build -p tiller_registry` before assuming the layering held.
- All blocking work (`ureq` fetches, `npm install`, unpacking) is owned by `tiller`/`main.rs` on GPUI's background executor. `tiller_ui` takes `tiller_registry` for **types only** — no `RegistryClient`, no `Installer`, no filesystem writes.
- Tests first. Standard Rust `#[test]`; red before green; commit per task.
- Conventional Commits, lower-case imperative subject.
- `Scripts/ci.sh` must print `CI OK` before the plan is done.
- User-facing strings in English.
- Never write outside the Tiller-owned data directory. Never touch user-global agent config (`~/.claude/settings.json`, `~/.codex/config.toml`).
- `npm install` runs with `--ignore-scripts`.
- Every `archive` URL must be `https`; validated at decode.
- Unpacking rejects any non-regular, non-directory entry and any path that does not resolve inside the destination.
- The executable bit is set on the artifact's `cmd` path only.
- The registry cache is written temp + `rename`. The install manifest rewrite is the commit point of an install.
- `OhMyPiAdapter::builtin_acp` stays `None` until a real `omp` answers an ACP `initialize` handshake. This is a ship gate, not a note.

### Dependency availability (measured 2026-08-23)

Already in the local cargo registry, so an offline build resolves them: `ureq 3.4.0`, `sha2 0.10.9`, `zip 8.6.0`, `flate2 1.1.9`. **`tar` is absent** — the first build after Task 5 needs network access once. Pin what the registry already holds rather than taking a newer major.

### Scope decisions carried from the spec

- `uvx` distribution: rejected with a named error, not implemented.
- `.tar.bz2` (only `goose`, 4 artifacts): rejected with the same named error as `uvx`. Adding a `bzip2` dependency for one agent is not worth it; the error names the format so the gap is visible rather than mysterious.
- Bare-executable artifacts (`sigit`, 4 artifacts) **are** implemented — that path has no unpacking step, so treating every artifact as an archive would fail outright.

---

## File Structure

**Create**

- `rust/crates/tiller_registry/Cargo.toml`
- `rust/crates/tiller_registry/src/lib.rs` — re-exports only; no logic.
- `rust/crates/tiller_registry/src/model.rs` — the registry payload and its decode/validate rules.
- `rust/crates/tiller_registry/src/resolve.rs` — `LaunchSource`, `ResolveInput`, `resolve()`, `registry_id()`. Pure; no I/O.
- `rust/crates/tiller_registry/src/client.rs` — fetch, validate, cache.
- `rust/crates/tiller_registry/src/store.rs` — the install manifest on disk.
- `rust/crates/tiller_registry/src/installer.rs` — staging, unpacking, integrity, commit.
- `rust/crates/tiller_registry/tests/fixtures/registry-v1.json` — the real published registry, committed.
- `rust/crates/tiller_registry/tests/fixtures/registry-unknown-distribution.json`
- `rust/crates/tiller_registry/tests/fixtures/registry-http-archive.json`
- `rust/crates/tiller_agents/tests/acp_conformance.rs` — the gated live handshake test.

**Modify**

- `rust/Cargo.toml` — add the member and the workspace dependency entry.
- `rust/crates/tiller_agents/src/lib.rs` — add `builtin_acp()`; later remove `acp_program`.
- `rust/crates/tiller_agents/src/{opencode,omp,claude,codex,pi}.rs` — implement `builtin_acp`; later drop `acp_program`.
- `rust/crates/tiller_ui/src/chat.rs` — remove the two hardcoded `npx` defaults.
- `rust/crates/tiller_ui/src/settings.rs` — render `LaunchSource`, version, Install/Update.
- `rust/crates/tiller/src/main.rs` — own the client/store/installer; resolve before launching a chat.
- `CLAUDE.md` — the crate-boundary diagram.

Each module is one responsibility and stays small enough to hold in context: `model` decodes, `resolve` decides, `client` caches, `store` remembers, `installer` acts.

---

### Task 1: The crate, and the registry payload

**Files:**
- Create: `rust/crates/tiller_registry/Cargo.toml`
- Create: `rust/crates/tiller_registry/src/lib.rs`
- Create: `rust/crates/tiller_registry/src/model.rs`
- Create: `rust/crates/tiller_registry/tests/fixtures/registry-v1.json`
- Create: `rust/crates/tiller_registry/tests/fixtures/registry-unknown-distribution.json`
- Create: `rust/crates/tiller_registry/tests/fixtures/registry-http-archive.json`
- Modify: `rust/Cargo.toml` (members list, ~line 9)

**Interfaces:**
- Produces: `AcpRegistry { version: String, agents: Vec<RegistryAgent>, warnings: Vec<String> }`, `AcpRegistry::from_json(&str) -> anyhow::Result<AcpRegistry>`, `RegistryAgent`, `Distribution`, `BinaryArtifact`, `AcpRegistry::agent(&self, id: &str) -> Option<&RegistryAgent>`. Every later task consumes these.

- [ ] **Step 1: Fetch and commit the real registry fixture**

```bash
cd rust/crates/tiller_registry && mkdir -p tests/fixtures
curl -sS --max-time 60 \
  -o tests/fixtures/registry-v1.json \
  https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json
```

This is a recorded fixture, not a live dependency: no test may fetch at runtime. If the agent count differs from 39, update the assertion in Step 3 to the count the fixture actually holds and say so in the commit message — the fixture is the truth, not this plan.

- [ ] **Step 2: Write the two synthetic fixtures**

`tests/fixtures/registry-unknown-distribution.json`:

```json
{
  "version": "1.0.0",
  "agents": [
    {
      "id": "future-agent",
      "name": "Future Agent",
      "version": "9.9.9",
      "distribution": { "flatpak": { "ref": "org.example.Future" } }
    }
  ]
}
```

`tests/fixtures/registry-http-archive.json`:

```json
{
  "version": "1.0.0",
  "agents": [
    {
      "id": "downgrade-agent",
      "name": "Downgrade Agent",
      "version": "1.0.0",
      "distribution": {
        "binary": {
          "linux-x86_64": {
            "archive": "http://example.invalid/agent-linux-x64.zip",
            "cmd": "./agent",
            "args": ["acp"]
          },
          "darwin-aarch64": {
            "archive": "https://example.invalid/agent-darwin-arm64.zip",
            "cmd": "./agent",
            "args": ["acp"]
          }
        }
      }
    }
  ]
}
```

- [ ] **Step 3: Write the failing tests**

Append to `rust/crates/tiller_registry/src/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_published_registry() {
        let registry =
            AcpRegistry::from_json(include_str!("../tests/fixtures/registry-v1.json"))
                .expect("decode the recorded registry");

        assert_eq!(registry.version, "1.0.0");
        assert_eq!(registry.agents.len(), 39);

        let opencode = registry.agent("opencode").expect("opencode row");
        let Some(Distribution::Binary(artifacts)) = opencode
            .distributions
            .iter()
            .find(|distribution| matches!(distribution, Distribution::Binary(_)))
        else {
            panic!("opencode publishes a binary kind, got {:?}", opencode.distributions);
        };
        let linux = artifacts.get("linux-x86_64").expect("linux-x86_64 artifact");
        assert_eq!(linux.cmd, "./opencode");
        assert_eq!(linux.args, vec!["acp".to_string()]);
        assert!(
            linux.sha256.is_some(),
            "opencode publishes a hash on every platform"
        );
    }

    #[test]
    fn an_artifact_without_a_hash_still_decodes() {
        // 47 of the registry's 95 binary artifacts publish no sha256. A
        // missing hash is a disclosure problem, not a decode error.
        let registry =
            AcpRegistry::from_json(include_str!("../tests/fixtures/registry-v1.json")).unwrap();
        let unhashed = registry.agents.iter().any(|agent| {
            agent.distributions.iter().any(|distribution| match distribution {
                Distribution::Binary(artifacts) => {
                    artifacts.values().any(|artifact| artifact.sha256.is_none())
                }
                _ => false,
            })
        });
        assert!(unhashed, "the recorded registry contains unhashed artifacts");
    }

    #[test]
    fn an_unknown_distribution_kind_decodes_rather_than_failing() {
        // The registry ships new distribution kinds without Tiller
        // releasing anything. A kind this build has never heard of must
        // not sink the whole document.
        let registry = AcpRegistry::from_json(include_str!(
            "../tests/fixtures/registry-unknown-distribution.json"
        ))
        .expect("an unknown kind is not a decode failure");

        let agent = registry.agent("future-agent").expect("row survives");
        assert_eq!(agent.distributions, vec![Distribution::Unknown]);
    }

    #[test]
    fn a_non_https_archive_is_dropped_with_a_warning() {
        // One bad URL must cost exactly one platform, not the document.
        let registry =
            AcpRegistry::from_json(include_str!("../tests/fixtures/registry-http-archive.json"))
                .expect("the document still decodes");

        let agent = registry.agent("downgrade-agent").expect("row survives");
        let Distribution::Binary(artifacts) = &agent.distribution else {
            panic!("expected a binary distribution");
        };
        assert!(
            !artifacts.contains_key("linux-x86_64"),
            "the http:// artifact is removed before anything can fetch it"
        );
        assert!(artifacts.contains_key("darwin-aarch64"), "the https one stays");
        assert_eq!(registry.warnings.len(), 1);
        assert!(registry.warnings[0].contains("downgrade-agent"));
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: FAIL — the package does not exist yet.

- [ ] **Step 5: Create the crate manifest**

`rust/crates/tiller_registry/Cargo.toml`:

```toml
[package]
name = "tiller_registry"
version.workspace = true
edition.workspace = true
publish.workspace = true

# Deliberately a leaf: no `tiller_*` dependency may appear here. The
# launch-resolution design puts the join with `tiller_agents` in the
# caller (see `resolve`), precisely so neither crate learns about the
# other.
[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
```

- [ ] **Step 6: Add the member to the workspace**

In `rust/Cargo.toml`, add to `members` (keep the existing order convention, appending after `crates/tiller_usage`):

```toml
    "crates/tiller_usage",
    "crates/tiller_registry",
]
```

And in `[workspace.dependencies]`, beside the other path entries:

```toml
tiller_registry = { path = "crates/tiller_registry" }
```

- [ ] **Step 7: Write `model.rs`**

```rust
//! The ACP registry payload, decoded defensively.
//!
//! Two rules shape this module, and both exist because the registry is a
//! document that changes without Tiller releasing anything:
//! an unrecognised distribution kind decodes as [`Distribution::Unknown`]
//! rather than failing, and a single malformed artifact costs one platform
//! rather than the whole document.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One decoded registry document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpRegistry {
    pub version: String,
    pub agents: Vec<RegistryAgent>,
    /// Entries dropped during validation, in human-readable form. Never an
    /// error: the caller renders these, the document still loads.
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryAgent {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub website: Option<String>,
    pub license: Option<String>,
    pub icon: Option<String>,
    /// An agent may declare several kinds — verified: `kilo` and `sigit`
    /// each publish both `binary` and `npx`. Position carries no meaning;
    /// `resolve` picks by kind and platform, never by index.
    pub distributions: Vec<Distribution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Distribution {
    Npx { package: String, args: Vec<String> },
    Binary(BTreeMap<String, BinaryArtifact>),
    Uvx { package: String, args: Vec<String> },
    /// A kind this build does not implement — including one published after
    /// it was compiled.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryArtifact {
    pub archive: String,
    pub cmd: String,
    pub args: Vec<String>,
    /// Absent on 47 of the 95 artifacts published today. `None` means "not
    /// published", never "not checked".
    pub sha256: Option<String>,
}

#[derive(Deserialize)]
struct AgentWire {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    distribution: DistributionWire,
}

/// Every field optional: a document carrying only kinds this build has
/// never seen leaves all of them `None`, which is exactly
/// [`Distribution::Unknown`].
#[derive(Default, Deserialize)]
struct DistributionWire {
    #[serde(default)]
    npx: Option<PackageWire>,
    #[serde(default)]
    binary: Option<BTreeMap<String, ArtifactWire>>,
    #[serde(default)]
    uvx: Option<PackageWire>,
}

#[derive(Deserialize)]
struct PackageWire {
    package: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Deserialize)]
struct ArtifactWire {
    archive: String,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Deserialize)]
struct RegistryWire {
    version: String,
    #[serde(default)]
    agents: Vec<AgentWire>,
}

impl AcpRegistry {
    /// Decodes and validates a registry document.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let wire: RegistryWire = serde_json::from_str(json)?;
        let mut warnings = Vec::new();
        let agents = wire
            .agents
            .into_iter()
            .map(|agent| {
                let distribution = decode_distribution(&agent.id, agent.distribution, &mut warnings);
                RegistryAgent {
                    id: agent.id,
                    name: agent.name,
                    version: agent.version,
                    description: agent.description,
                    repository: agent.repository,
                    website: agent.website,
                    license: agent.license,
                    icon: agent.icon,
                    distribution,
                }
            })
            .collect();
        Ok(Self { version: wire.version, agents, warnings })
    }

    pub fn agent(&self, id: &str) -> Option<&RegistryAgent> {
        self.agents.iter().find(|agent| agent.id == id)
    }
}

fn decode_distribution(
    agent_id: &str,
    wire: DistributionWire,
    warnings: &mut Vec<String>,
) -> Distribution {
    if let Some(npx) = wire.npx {
        return Distribution::Npx { package: npx.package, args: npx.args };
    }
    if let Some(binary) = wire.binary {
        let artifacts: BTreeMap<String, BinaryArtifact> = binary
            .into_iter()
            .filter_map(|(platform, artifact)| {
                // Rejected before anything can fetch it: an `http://` or
                // `file://` archive is a downgrade, not a variation.
                if !artifact.archive.starts_with("https://") {
                    warnings.push(format!(
                        "{agent_id}: dropped {platform} artifact with non-https archive URL"
                    ));
                    return None;
                }
                Some((
                    platform,
                    BinaryArtifact {
                        archive: artifact.archive,
                        cmd: artifact.cmd,
                        args: artifact.args,
                        sha256: artifact.sha256,
                    },
                ))
            })
            .collect();
        return Distribution::Binary(artifacts);
    }
    if let Some(uvx) = wire.uvx {
        return Distribution::Uvx { package: uvx.package, args: uvx.args };
    }
    Distribution::Unknown
}
```

- [ ] **Step 8: Write `lib.rs`**

```rust
//! The ACP agent registry: what agents exist, which are installed, and how
//! to install one.
//!
//! This crate is a leaf by design — see `Cargo.toml`.

mod model;

pub use model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
```

- [ ] **Step 9: Add `serde` to the workspace dependency table**

`serde` is already in `[workspace.dependencies]` with `features = ["derive"]`. Confirm before adding a duplicate:

```bash
cd rust && grep -n '^serde' Cargo.toml
```

- [ ] **Step 10: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 4 tests.

- [ ] **Step 11: Verify the leaf boundary held**

```bash
cd rust && cargo tree -p tiller_registry --depth 1 | grep -i tiller
```

Expected: only `tiller_registry` itself. Any other `tiller_*` line is a layering failure — fix it before committing.

- [ ] **Step 12: Commit**

```bash
git add rust/Cargo.toml rust/crates/tiller_registry
git commit -m "feat(registry): decode the acp registry payload defensively"
```

---

### Task 2: The launch ladder, as a pure function

**Files:**
- Create: `rust/crates/tiller_registry/src/resolve.rs`
- Modify: `rust/crates/tiller_registry/src/lib.rs`

**Interfaces:**
- Consumes: `AcpRegistry`, `RegistryAgent`, `Distribution` (Task 1).
- Produces: `LaunchSource`, `UnavailableReason`, `ResolveInput<'a>`, `InstalledAgent`, `Integrity`, `BuiltinAcp`, `resolve(ResolveInput) -> LaunchSource`, `registry_id(&str) -> Option<&'static str>`, `current_platform_key() -> &'static str`. Tasks 4, 5, 8 and 9 consume these. `InstalledAgent` and `Integrity` are defined here rather than in `store.rs` because `resolve` is the only thing the store, the installer and the UI all have in common.

- [ ] **Step 1: Write the failing tests**

Append to `rust/crates/tiller_registry/src/resolve.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn builtin() -> BuiltinAcp {
        BuiltinAcp { program: "opencode", args: &["acp"] }
    }

    fn installed() -> InstalledAgent {
        InstalledAgent {
            id: "opencode".into(),
            version: "1.18.21".into(),
            executable: PathBuf::from("/data/tiller/agents/opencode/1.18.21/opencode"),
            args: vec!["acp".into()],
            integrity: Integrity::Sha256,
        }
    }

    fn registry_with(id: &str, distributions: Vec<Distribution>) -> AcpRegistry {
        AcpRegistry {
            version: "1.0.0".into(),
            warnings: Vec::new(),
            agents: vec![RegistryAgent {
                id: id.into(),
                name: id.into(),
                version: "1.0.0".into(),
                description: None,
                repository: None,
                website: None,
                license: None,
                icon: None,
                distributions,
            }],
        }
    }

    fn binary_for(platform: &str) -> Distribution {
        let mut artifacts = BTreeMap::new();
        artifacts.insert(
            platform.to_string(),
            BinaryArtifact {
                archive: "https://example.invalid/a.zip".into(),
                cmd: "./a".into(),
                args: vec!["acp".into()],
                sha256: Some("00".repeat(32)),
            },
        );
        Distribution::Binary(artifacts)
    }

    fn input<'a>(registry: Option<&'a AcpRegistry>) -> ResolveInput<'a> {
        ResolveInput {
            adapter_id: "opencode",
            builtin: None,
            builtin_on_path: false,
            installed: None,
            registry,
            platform_key: "linux-x86_64",
        }
    }

    #[test]
    fn a_builtin_on_path_wins_over_everything() {
        let registry = registry_with("opencode", vec![binary_for("linux-x86_64")]);
        let source = resolve(ResolveInput {
            builtin: Some(builtin()),
            builtin_on_path: true,
            installed: Some((installed(), true)),
            ..input(Some(&registry))
        });
        assert_eq!(
            source,
            LaunchSource::Builtin { program: "opencode".into(), args: vec!["acp".into()] },
            "the user's own CLI is used, and nothing is downloaded"
        );
    }

    #[test]
    fn a_builtin_claim_without_the_binary_falls_through() {
        // The trap this signature exists to prevent: claiming `Builtin` for
        // a binary that is not there shadows a working managed copy and
        // fails at exec with no way down the ladder.
        let source = resolve(ResolveInput {
            builtin: Some(builtin()),
            builtin_on_path: false,
            installed: Some((installed(), true)),
            ..input(None)
        });
        assert_eq!(source, LaunchSource::Installed(installed()));
    }

    #[test]
    fn a_stale_manifest_offers_a_reinstall_rather_than_failing() {
        let registry = registry_with("opencode", vec![binary_for("linux-x86_64")]);
        let source = resolve(ResolveInput {
            installed: Some((installed(), false)),
            ..input(Some(&registry))
        });
        match source {
            LaunchSource::Installable { agent } => assert_eq!(agent.id, "opencode"),
            other => panic!("expected Installable, got {other:?}"),
        }
    }

    #[test]
    fn a_platform_without_an_artifact_says_so() {
        let registry = registry_with("opencode", vec![binary_for("darwin-aarch64")]);
        assert_eq!(
            resolve(input(Some(&registry))),
            LaunchSource::Unavailable(UnavailableReason::NoArtifactForPlatform),
            "at least two registry agents have no linux-aarch64 artifact; the row must say which"
        );
    }

    #[test]
    fn an_unknown_distribution_is_unavailable_not_installable() {
        let registry = registry_with("opencode", vec![Distribution::Unknown]);
        assert_eq!(
            resolve(input(Some(&registry))),
            LaunchSource::Unavailable(UnavailableReason::UnsupportedDistribution)
        );
    }

    #[test]
    fn an_agent_the_registry_does_not_carry_is_unavailable() {
        let registry = registry_with("something-else", vec![binary_for("linux-x86_64")]);
        assert_eq!(
            resolve(input(Some(&registry))),
            LaunchSource::Unavailable(UnavailableReason::NotInRegistry)
        );
    }

    #[test]
    fn adapter_ids_map_onto_registry_ids_explicitly() {
        // The two namespaces do not coincide, and guessing between them is
        // the same class of bug `resolveBinName` exists to prevent.
        assert_eq!(registry_id("claude"), Some("claude-acp"));
        assert_eq!(registry_id("codex"), Some("codex-acp"));
        assert_eq!(registry_id("pi"), Some("pi-acp"));
        assert_eq!(registry_id("opencode"), Some("opencode"));
        assert_eq!(registry_id("omp"), None, "omp is not in the registry at all");
        assert_eq!(registry_id("not-an-adapter"), None);
    }

    #[test]
    fn a_registry_agent_is_looked_up_by_its_own_id_when_no_adapter_maps_to_it() {
        let registry = registry_with("github-copilot-cli", vec![binary_for("linux-x86_64")]);
        let source = resolve(ResolveInput {
            adapter_id: "github-copilot-cli",
            ..input(Some(&registry))
        });
        match source {
            LaunchSource::Installable { agent } => assert_eq!(agent.id, "github-copilot-cli"),
            other => panic!("expected Installable, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry resolve
```

Expected: FAIL — `resolve.rs` is not a module yet.

- [ ] **Step 3: Write `resolve.rs`**

```rust
//! Which of the four ways to launch an agent applies, decided from facts
//! rather than claims.
//!
//! This function performs no I/O on purpose. Its caller answers the two
//! existence questions — is the built-in binary on PATH, does the recorded
//! executable still exist — and passes the answers in. An earlier draft
//! took only the static claim, which meant an uninstalled CLI still
//! resolved to `Builtin`, shadowed a working managed copy, and failed at
//! `exec` with nowhere to fall back to.

use std::path::PathBuf;

use crate::model::{AcpRegistry, Distribution, RegistryAgent};

/// An adapter's in-binary ACP claim: a subcommand of a CLI the user already
/// has. Supplied by the caller from `tiller_agents`, which this crate does
/// not depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinAcp {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

/// Whether an install could verify what it downloaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Integrity {
    /// The registry published a hash and it matched.
    Sha256,
    /// The registry published no hash for this artifact. Recorded so an
    /// unverified install stays auditable afterwards.
    None,
}

/// One agent installed and owned by Tiller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledAgent {
    pub id: String,
    pub version: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub integrity: Integrity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    /// The registry carries the agent, but publishes nothing for this
    /// platform. Measured: `linux-aarch64` has 16 artifacts against
    /// `linux-x86_64`'s 18.
    NoArtifactForPlatform,
    /// `uvx`, `.tar.bz2`, or a kind published after this build.
    UnsupportedDistribution,
    NotInRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchSource {
    Builtin { program: String, args: Vec<String> },
    Installed(InstalledAgent),
    Installable { agent: RegistryAgent },
    Unavailable(UnavailableReason),
}

pub struct ResolveInput<'a> {
    pub adapter_id: &'a str,
    pub builtin: Option<BuiltinAcp>,
    /// Whether `builtin.program` resolves on PATH right now.
    /// `tiller_agents::availability()` already computes this.
    pub builtin_on_path: bool,
    /// The recorded install, and whether its executable still exists.
    pub installed: Option<(InstalledAgent, bool)>,
    pub registry: Option<&'a AcpRegistry>,
    pub platform_key: &'a str,
}

/// The join between adapter ids and registry ids. An explicit table, never
/// name matching: `claude` the CLI and `claude-acp` the wrapper are
/// different packages, and a substring heuristic here would reintroduce
/// exactly the silently-wrong-program bug `resolveBinName` prevents.
pub fn registry_id(adapter_id: &str) -> Option<&'static str> {
    match adapter_id {
        "claude" => Some("claude-acp"),
        "codex" => Some("codex-acp"),
        "pi" => Some("pi-acp"),
        "opencode" => Some("opencode"),
        // Oh-My-Pi is not in the registry; it is reachable only through its
        // own `omp acp` subcommand.
        "omp" => None,
        _ => None,
    }
}

pub fn resolve(input: ResolveInput<'_>) -> LaunchSource {
    if let Some(builtin) = input.builtin
        && input.builtin_on_path
    {
        return LaunchSource::Builtin {
            program: builtin.program.to_string(),
            args: builtin.args.iter().map(|arg| (*arg).to_string()).collect(),
        };
    }

    if let Some((installed, executable_exists)) = input.installed
        && executable_exists
    {
        return LaunchSource::Installed(installed);
    }

    let Some(registry) = input.registry else {
        return LaunchSource::Unavailable(UnavailableReason::NotInRegistry);
    };

    // An adapter uses its mapped registry id; a bare registry agent uses its
    // own.
    let lookup = registry_id(input.adapter_id).unwrap_or(input.adapter_id);
    let Some(agent) = registry.agent(lookup) else {
        return LaunchSource::Unavailable(UnavailableReason::NotInRegistry);
    };

    // An agent may declare several distribution kinds — verified against the
    // published registry: `kilo` and `sigit` each carry both `binary` and
    // `npx`. `model.rs` deliberately does not pick between them, because it
    // has no platform context: `binary` is preferable where an artifact
    // exists for this machine, but on a platform the agent does not build
    // for, `npx` is the only thing that works. Choosing here, where the
    // platform is known, is the whole reason the model keeps a Vec.
    let installable_binary = agent.distributions.iter().any(|distribution| {
        matches!(distribution, Distribution::Binary(artifacts)
            if artifacts.contains_key(input.platform_key))
    });
    let has_npx = agent
        .distributions
        .iter()
        .any(|distribution| matches!(distribution, Distribution::Npx { .. }));
    if installable_binary || has_npx {
        return LaunchSource::Installable { agent: agent.clone() };
    }

    // Nothing installable: say which kind of "no" this is.
    let declares_binary = agent
        .distributions
        .iter()
        .any(|distribution| matches!(distribution, Distribution::Binary(_)));
    if declares_binary {
        LaunchSource::Unavailable(UnavailableReason::NoArtifactForPlatform)
    } else {
        LaunchSource::Unavailable(UnavailableReason::UnsupportedDistribution)
    }
}

/// The platform key this build runs on, in the registry's spelling.
pub fn current_platform_key() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "x86_64") => "darwin-x86_64",
        ("macos", "aarch64") => "darwin-aarch64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        _ => "unsupported",
    }
}
```

- [ ] **Step 4: Export from `lib.rs`**

```rust
mod model;
mod resolve;

pub use model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
pub use resolve::{
    BuiltinAcp, InstalledAgent, Integrity, LaunchSource, ResolveInput, UnavailableReason,
    current_platform_key, registry_id, resolve,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 12 tests.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_registry/src
git commit -m "feat(registry): rank launch sources from existence facts"
```

---

### Task 3: The registry client and its cache

**Files:**
- Create: `rust/crates/tiller_registry/src/client.rs`
- Modify: `rust/crates/tiller_registry/src/lib.rs`
- Modify: `rust/crates/tiller_registry/Cargo.toml`

**Interfaces:**
- Consumes: `AcpRegistry::from_json` (Task 1).
- Produces: `RegistryClient::new(cache_path, fetch, now)`, `RegistryClient::with_http(cache_path)`, `RegistryClient::registry(&self, max_age: Duration, force: bool) -> anyhow::Result<AcpRegistry>`, `RegistryClient::REGISTRY_URL`, `RegistryClient::DEFAULT_MAX_AGE`. Task 8 constructs this.

- [ ] **Step 1: Write the failing tests**

Append to `rust/crates/tiller_registry/src/client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const GOOD: &str = r#"{"version":"1.0.0","agents":[
        {"id":"a","name":"A","version":"1.0.0",
         "distribution":{"npx":{"package":"a@1.0.0"}}}]}"#;
    const OTHER: &str = r#"{"version":"2.0.0","agents":[]}"#;

    fn temp_cache(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tiller-registry-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("registry.json")
    }

    #[test]
    fn a_fresh_cache_is_served_without_touching_the_network() {
        let cache = temp_cache("fresh");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache,
            // The panic is the assertion: reaching the network at all is
            // the failure.
            || panic!("the network must not be touched for a fresh cache"),
            SystemTime::now,
        );
        let registry = client.registry(Duration::from_secs(86_400), false).unwrap();
        assert_eq!(registry.version, "1.0.0");
    }

    #[test]
    fn a_failed_fetch_falls_back_to_the_cached_copy() {
        let cache = temp_cache("offline");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache,
            || Err(anyhow::anyhow!("no network")),
            SystemTime::now,
        );
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(
            registry.version, "1.0.0",
            "an installed agent still launches offline; that is the point of installing"
        );
    }

    #[test]
    fn an_invalid_payload_never_clobbers_a_good_cache() {
        let cache = temp_cache("clobber");
        std::fs::write(&cache, GOOD).unwrap();
        let client = RegistryClient::new(
            cache.clone(),
            || Ok("{ this is not json".to_string()),
            SystemTime::now,
        );
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(registry.version, "1.0.0");
        assert_eq!(
            std::fs::read_to_string(&cache).unwrap(),
            GOOD,
            "the good cache is still on disk byte for byte"
        );
    }

    #[test]
    fn a_successful_fetch_replaces_the_cache() {
        let cache = temp_cache("replace");
        std::fs::write(&cache, GOOD).unwrap();
        let client =
            RegistryClient::new(cache.clone(), || Ok(OTHER.to_string()), SystemTime::now);
        let registry = client.registry(Duration::ZERO, true).unwrap();
        assert_eq!(registry.version, "2.0.0");
        assert_eq!(std::fs::read_to_string(&cache).unwrap(), OTHER);
        assert!(
            !cache.with_extension("json.tmp").exists(),
            "the temp file used for the atomic write is gone"
        );
    }

    #[test]
    fn a_corrupt_cache_is_treated_as_absent_rather_than_fatal() {
        // A build killed mid-write, or a hand-edited file, must not make
        // the Agents screen permanently unopenable.
        let cache = temp_cache("corrupt");
        std::fs::write(&cache, "{ truncated").unwrap();
        let client =
            RegistryClient::new(cache, || Ok(GOOD.to_string()), SystemTime::now);
        let registry = client.registry(Duration::from_secs(86_400), false).unwrap();
        assert_eq!(registry.version, "1.0.0");
    }

    #[test]
    fn no_cache_and_no_network_is_an_error_not_a_fabricated_registry() {
        let cache = temp_cache("nothing");
        let _ = std::fs::remove_file(&cache);
        let client =
            RegistryClient::new(cache, || Err(anyhow::anyhow!("no network")), SystemTime::now);
        assert!(client.registry(Duration::ZERO, true).is_err());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry client
```

Expected: FAIL — no `client` module.

- [ ] **Step 3: Add the HTTP dependency**

In `rust/crates/tiller_registry/Cargo.toml`:

```toml
# Blocking on purpose: this crate does one 50 KB GET a day plus the
# occasional download, and a second async runtime beside GPUI's and
# tiller_acp's would cost more than it buys. `tiller` runs these calls on
# the background executor.
ureq = "3.4"
```

- [ ] **Step 4: Write `client.rs`**

```rust
//! Fetching the registry, with a disk cache that survives being offline.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::model::AcpRegistry;

type Fetch = Box<dyn Fn() -> anyhow::Result<String> + Send + Sync>;
type Clock = Box<dyn Fn() -> SystemTime + Send + Sync>;

pub struct RegistryClient {
    cache_path: PathBuf,
    fetch: Fetch,
    now: Clock,
}

impl RegistryClient {
    pub const REGISTRY_URL: &'static str =
        "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
    pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(86_400);

    /// `fetch` and `now` are injected so every rule below is a unit test
    /// with no network and no wall clock.
    pub fn new(
        cache_path: impl Into<PathBuf>,
        fetch: impl Fn() -> anyhow::Result<String> + Send + Sync + 'static,
        now: impl Fn() -> SystemTime + Send + Sync + 'static,
    ) -> Self {
        Self { cache_path: cache_path.into(), fetch: Box::new(fetch), now: Box::new(now) }
    }

    /// The production constructor.
    pub fn with_http(cache_path: impl Into<PathBuf>) -> Self {
        Self::new(
            cache_path,
            || {
                let body = ureq::get(Self::REGISTRY_URL)
                    .config()
                    .timeout_global(Some(Duration::from_secs(30)))
                    .build()
                    .call()?
                    .body_mut()
                    .read_to_string()?;
                Ok(body)
            },
            SystemTime::now,
        )
    }

    pub fn registry(&self, max_age: Duration, force: bool) -> anyhow::Result<AcpRegistry> {
        if !force
            && let Some(cached) = self.fresh_cache(max_age)
        {
            return Ok(cached);
        }

        match (self.fetch)() {
            Ok(body) => match AcpRegistry::from_json(&body) {
                Ok(registry) => {
                    // Validated first: a bad payload never clobbers a good
                    // cache.
                    self.write_cache(&body)?;
                    Ok(registry)
                }
                Err(_) => self
                    .cached()
                    .ok_or_else(|| anyhow::anyhow!("registry payload did not decode")),
            },
            Err(error) => self.cached().ok_or(error),
        }
    }

    fn fresh_cache(&self, max_age: Duration) -> Option<AcpRegistry> {
        let modified = std::fs::metadata(&self.cache_path).ok()?.modified().ok()?;
        let age = (self.now)().duration_since(modified).ok()?;
        (age < max_age).then(|| self.cached())?
    }

    /// A cache that fails to decode is absent, not fatal.
    fn cached(&self) -> Option<AcpRegistry> {
        let body = std::fs::read_to_string(&self.cache_path).ok()?;
        AcpRegistry::from_json(&body).ok()
    }

    /// Temp + rename: this file is read every time the Agents screen opens,
    /// so a process killed mid-write must not leave a truncated document
    /// behind.
    fn write_cache(&self, body: &str) -> anyhow::Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = self.cache_path.with_extension("json.tmp");
        std::fs::write(&temp, body)?;
        std::fs::rename(&temp, &self.cache_path)?;
        Ok(())
    }
}
```

- [ ] **Step 5: Export from `lib.rs`**

Add `mod client;` and `pub use client::RegistryClient;`.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 18 tests. If `ureq`'s builder API differs from the snippet above, adapt the call and keep the 30-second global timeout — the timeout is the requirement, the spelling is not.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/tiller_registry
git commit -m "feat(registry): cache the registry so being offline is survivable"
```

---

### Task 4: The install store

**Files:**
- Create: `rust/crates/tiller_registry/src/store.rs`
- Modify: `rust/crates/tiller_registry/src/lib.rs`

**Interfaces:**
- Consumes: `InstalledAgent`, `Integrity` (Task 2).
- Produces: `InstallStore::new(root: PathBuf)`, `InstallStore::default_root(env: &BTreeMap<String, String>) -> PathBuf`, `InstallStore::manifest(&self, id: &str) -> Option<InstalledAgent>`, `InstallStore::write(&self, agent: &InstalledAgent) -> anyhow::Result<()>`, `InstallStore::remove(&self, id: &str) -> anyhow::Result<()>`, `InstallStore::root(&self) -> &Path`. Tasks 5, 6 and 8 consume these.

**Note on the XDG helper.** The spec preferred hoisting a shared directory helper over duplicating `tiller_control`'s. On inspection the two are not the same function: `tiller_control::protocol::default_socket_path` (`protocol.rs:98-139`) resolves `$TILLER_SOCKET` first, then `$XDG_RUNTIME_DIR`, then the *state* directory, with a macOS branch; this store needs `$XDG_DATA_HOME` with no override and no runtime dir. A shared crate for ~15 lines that neither caller uses verbatim is not worth it — so take the spec's stated fallback: duplicate, with a comment in this copy naming the other. Do not do it silently.

- [ ] **Step 1: Write the failing tests**

Append to `rust/crates/tiller_registry/src/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::Integrity;
    use std::collections::BTreeMap;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("tiller-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn agent() -> InstalledAgent {
        InstalledAgent {
            id: "codex-acp".into(),
            version: "1.6.2".into(),
            executable: PathBuf::from("/data/codex-acp/1.6.2/node_modules/.bin/codex-acp"),
            args: vec![],
            integrity: Integrity::None,
        }
    }

    #[test]
    fn an_unknown_agent_has_no_manifest() {
        let store = InstallStore::new(temp_root("unknown"));
        assert_eq!(store.manifest("codex-acp"), None);
    }

    #[test]
    fn a_written_manifest_reads_back_identically() {
        let store = InstallStore::new(temp_root("roundtrip"));
        store.write(&agent()).unwrap();
        assert_eq!(store.manifest("codex-acp"), Some(agent()));
    }

    #[test]
    fn writing_twice_replaces_rather_than_appends() {
        let store = InstallStore::new(temp_root("replace"));
        store.write(&agent()).unwrap();
        let newer = InstalledAgent { version: "1.7.0".into(), ..agent() };
        store.write(&newer).unwrap();
        assert_eq!(store.manifest("codex-acp").unwrap().version, "1.7.0");
    }

    #[test]
    fn an_unreadable_manifest_is_absent_rather_than_fatal() {
        let root = temp_root("corrupt");
        let store = InstallStore::new(root.clone());
        std::fs::create_dir_all(root.join("codex-acp")).unwrap();
        std::fs::write(root.join("codex-acp/manifest.json"), "{ truncated").unwrap();
        assert_eq!(store.manifest("codex-acp"), None);
    }

    #[test]
    fn removing_an_agent_clears_its_manifest() {
        let store = InstallStore::new(temp_root("remove"));
        store.write(&agent()).unwrap();
        store.remove("codex-acp").unwrap();
        assert_eq!(store.manifest("codex-acp"), None);
    }

    #[test]
    fn the_default_root_follows_xdg_data_home() {
        let env = BTreeMap::from([("XDG_DATA_HOME".to_string(), "/data".to_string())]);
        assert_eq!(InstallStore::default_root(&env), PathBuf::from("/data/tiller/agents"));
    }

    #[test]
    fn the_default_root_falls_back_to_home_local_share() {
        let env = BTreeMap::from([("HOME".to_string(), "/home/enzo".to_string())]);
        assert_eq!(
            InstallStore::default_root(&env),
            PathBuf::from("/home/enzo/.local/share/tiller/agents")
        );
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        // The XDG spec says a relative value is invalid and must be treated
        // as unset — the same filter `tiller_control` applies.
        let env = BTreeMap::from([
            ("XDG_DATA_HOME".to_string(), "relative/path".to_string()),
            ("HOME".to_string(), "/home/enzo".to_string()),
        ]);
        assert_eq!(
            InstallStore::default_root(&env),
            PathBuf::from("/home/enzo/.local/share/tiller/agents")
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry store
```

Expected: FAIL — no `store` module.

- [ ] **Step 3: Write `store.rs`**

```rust
//! What Tiller has installed, where, and at which version.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::resolve::{InstalledAgent, Integrity};

pub struct InstallStore {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct ManifestWire {
    id: String,
    version: String,
    executable: PathBuf,
    #[serde(default)]
    args: Vec<String>,
    /// "sha256" or "none". Recorded so an unverified install stays
    /// auditable after the fact: 47 of the registry's 95 binary artifacts
    /// publish no hash.
    integrity: String,
}

impl InstallStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `$XDG_DATA_HOME/tiller/agents`, falling back to
    /// `$HOME/.local/share`.
    ///
    /// This deliberately duplicates the shape of
    /// `tiller_control::protocol::default_socket_path` (`protocol.rs:98-139`)
    /// rather than sharing it: that function resolves `$TILLER_SOCKET`
    /// first, then `$XDG_RUNTIME_DIR`, then the *state* directory, and
    /// carries a macOS branch — none of which applies here. The
    /// duplication is the absolute-path filter and nothing more; the other
    /// copy is named so a future change to XDG handling can find both.
    pub fn default_root(environment: &BTreeMap<String, String>) -> PathBuf {
        let data_home = absolute(environment, "XDG_DATA_HOME").unwrap_or_else(|| {
            absolute(environment, "HOME")
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".local/share")
        });
        data_home.join("tiller").join("agents")
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("manifest.json")
    }

    /// A manifest that does not read or does not decode is absent, not an
    /// error: `resolve` turns that into an offer to reinstall.
    pub fn manifest(&self, id: &str) -> Option<InstalledAgent> {
        let body = std::fs::read_to_string(self.manifest_path(id)).ok()?;
        let wire: ManifestWire = serde_json::from_str(&body).ok()?;
        Some(InstalledAgent {
            id: wire.id,
            version: wire.version,
            executable: wire.executable,
            args: wire.args,
            integrity: match wire.integrity.as_str() {
                "sha256" => Integrity::Sha256,
                _ => Integrity::None,
            },
        })
    }

    /// The commit point of an install. Temp + rename, so a crash here
    /// leaves the previously recorded version fully intact.
    pub fn write(&self, agent: &InstalledAgent) -> anyhow::Result<()> {
        let path = self.manifest_path(&agent.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let wire = ManifestWire {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: agent.executable.clone(),
            args: agent.args.clone(),
            integrity: match agent.integrity {
                Integrity::Sha256 => "sha256".to_string(),
                Integrity::None => "none".to_string(),
            },
        };
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&wire)?)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }

    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        let dir = self.root.join(id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

fn absolute(environment: &BTreeMap<String, String>, key: &str) -> Option<PathBuf> {
    environment
        .get(key)
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
}
```

- [ ] **Step 4: Add `serde` derive to the manifest**

`serde` is already a dependency from Task 1 with the `derive` feature from the workspace table. Confirm:

```bash
cd rust && cargo build -p tiller_registry
```

- [ ] **Step 5: Export from `lib.rs`**

Add `mod store;` and `pub use store::InstallStore;`.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 26 tests.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/tiller_registry
git commit -m "feat(registry): record what tiller installed and where"
```

---

### Task 5: Installing a binary artifact

**Files:**
- Create: `rust/crates/tiller_registry/src/installer.rs`
- Modify: `rust/crates/tiller_registry/src/lib.rs`
- Modify: `rust/crates/tiller_registry/Cargo.toml`

**Interfaces:**
- Consumes: `RegistryAgent`, `Distribution`, `BinaryArtifact` (Task 1), `InstalledAgent`, `Integrity` (Task 2), `InstallStore` (Task 4).
- Produces: `Installer::new(store: InstallStore)`, `Installer::install(&self, agent: &RegistryAgent, platform_key: &str) -> Result<InstalledAgent, InstallError>`, `InstallError`, `Installer::sweep_staging(&self) -> anyhow::Result<()>`, `unpack_kind(url: &str) -> UnpackKind`. Tasks 6 and 8 consume these.

- [ ] **Step 1: Write the failing tests**

Append to `rust/crates/tiller_registry/src/installer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unpack_kind_comes_from_the_url() {
        // Measured across the registry's 95 artifacts: tar.gz 55, zip 32,
        // tar.bz2 4, bare executable 4. The bare case has no unpacking step
        // at all, so it cannot be an afterthought.
        assert_eq!(unpack_kind("https://x/opencode-linux-x64.zip"), UnpackKind::Zip);
        assert_eq!(unpack_kind("https://x/agent-linux.tar.gz"), UnpackKind::TarGz);
        assert_eq!(unpack_kind("https://x/agent-linux.tgz"), UnpackKind::TarGz);
        assert_eq!(
            unpack_kind("https://x/goose-x86_64-unknown-linux-gnu.tar.bz2"),
            UnpackKind::Unsupported
        );
        assert_eq!(unpack_kind("https://x/sigit-linux-amd64"), UnpackKind::BareExecutable);
        assert_eq!(unpack_kind("https://x/sigit-win-amd64.exe"), UnpackKind::BareExecutable);
    }

    #[test]
    fn a_mismatched_hash_aborts_the_install() {
        let bytes = b"payload";
        assert!(verify_sha256(bytes, Some(&"ff".repeat(32))).is_err());
    }

    #[test]
    fn a_matching_hash_passes_and_is_recorded() {
        let bytes = b"payload";
        let digest = sha256_hex(bytes);
        assert_eq!(verify_sha256(bytes, Some(&digest)).unwrap(), Integrity::Sha256);
    }

    #[test]
    fn an_absent_hash_installs_but_is_recorded_as_unverified() {
        // 9 of the 18 binary agents publish at least one artifact with no
        // hash. Refusing them buys no safety — the fallback is downloading
        // the same file by hand — so the fact is recorded instead.
        assert_eq!(verify_sha256(b"payload", None).unwrap(), Integrity::None);
    }

    #[test]
    fn an_entry_escaping_the_destination_is_refused() {
        let dest = std::path::Path::new("/data/staging");
        assert!(safe_entry_path(dest, std::path::Path::new("../../etc/passwd")).is_err());
        assert!(safe_entry_path(dest, std::path::Path::new("/etc/passwd")).is_err());
        assert!(safe_entry_path(dest, std::path::Path::new("bin/agent")).is_ok());
    }

    #[test]
    fn a_staging_directory_is_named_for_its_owner() {
        let staging = staging_dir(std::path::Path::new("/data"), "codex-acp", "1.6.2");
        let name = staging.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("codex-acp-1.6.2-"));
        assert!(
            name.ends_with(&std::process::id().to_string()),
            "the pid lets a startup sweep tell live staging from abandoned"
        );
        assert_eq!(staging.parent().unwrap(), std::path::Path::new("/data/.staging"));
    }

    #[test]
    fn sweeping_removes_staging_left_by_a_dead_process() {
        let root = std::env::temp_dir()
            .join(format!("tiller-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dead = root.join(".staging/codex-acp-1.6.2-1");
        let live = staging_dir(&root, "codex-acp", "1.6.2");
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::create_dir_all(&live).unwrap();

        Installer::new(InstallStore::new(root.clone())).sweep_staging().unwrap();

        assert!(!dead.exists(), "abandoned staging is collected");
        assert!(live.exists(), "this process's own staging is left alone");
    }

    #[test]
    fn sweeping_also_clears_a_lock_left_by_a_killed_install() {
        // Without this, one killed install makes that agent permanently
        // uninstallable: `InstallGuard::acquire` uses create_new, so the
        // orphaned lock rejects every later attempt with AlreadyRunning.
        let root = std::env::temp_dir()
            .join(format!("tiller-sweep-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lock = root.join(".staging/codex-acp.lock");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "").unwrap();

        Installer::new(InstallStore::new(root.clone())).sweep_staging().unwrap();

        assert!(!lock.exists(), "a lock that outlived its process is stale");
    }

    #[test]
    fn an_unsupported_archive_names_the_format() {
        let error = InstallError::UnsupportedArchive {
            agent: "goose".into(),
            url: "https://x/goose.tar.bz2".into(),
        };
        let rendered = error.to_string();
        assert!(rendered.contains("goose"));
        assert!(rendered.contains("tar.bz2"), "the gap is visible, not mysterious");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry installer
```

Expected: FAIL — no `installer` module.

**Note on the `ureq` calls below.** This task and Task 3 spell the `ureq` response API differently, and at most one of them matches ureq 3.4. Verify the real API once and use one consistent form across both modules. The binding requirements are the ceilings and timeouts — a 600-second global timeout and a 512 MB download cap here, 30 seconds on the registry fetch in Task 3 — not the method names.

- [ ] **Step 3: Add the unpacking dependencies**

In `rust/crates/tiller_registry/Cargo.toml`. `tar` is the one crate not already in the local registry, so this build needs network once:

```toml
sha2 = "0.10"
zip = "8"
tar = "0.4"
flate2 = "1"
```

- [ ] **Step 4: Write `installer.rs`**

```rust
//! Installing one agent: download, verify what can be verified, unpack
//! safely, and commit by rewriting the manifest.
//!
//! The commit point matters. `rename` onto a non-empty directory fails on
//! Linux, so a directory swap is necessarily remove-then-move and a crash
//! inside that window leaves *nothing*. Installing into
//! `<root>/<id>/<version>/` and letting the manifest name the version turns
//! the commit into a single-file rename, which is genuinely atomic: a crash
//! anywhere before it leaves the previously recorded version fully intact.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::model::{BinaryArtifact, Distribution, RegistryAgent};
use crate::resolve::{InstalledAgent, Integrity};
use crate::store::InstallStore;

/// Ceilings, so a hostile or broken artifact cannot fill the disk or hang
/// the row forever. The largest artifact in the registry today is tens of
/// megabytes.
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{agent}: no artifact published for this platform")]
    NoArtifactForPlatform { agent: String },
    #[error("{agent}: archive format is not supported ({url})")]
    UnsupportedArchive { agent: String, url: String },
    #[error("{agent}: distribution kind is not supported")]
    UnsupportedDistribution { agent: String },
    #[error("{agent}: checksum did not match; nothing was installed")]
    ChecksumMismatch { agent: String },
    #[error("{agent}: archive entry {entry} escapes the destination")]
    UnsafeArchiveEntry { agent: String, entry: String },
    #[error("{agent}: {cmd} is missing from the unpacked artifact")]
    MissingCommand { agent: String, cmd: String },
    #[error("{agent}: another install is already running")]
    AlreadyRunning { agent: String },
    #[error("{agent}: {message}")]
    Failed { agent: String, message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackKind {
    Zip,
    TarGz,
    /// Not an archive: `sigit` publishes bare executables.
    BareExecutable,
    /// `.tar.bz2` (goose). Named rather than silently skipped.
    Unsupported,
}

pub struct Installer {
    store: InstallStore,
}

pub fn unpack_kind(url: &str) -> UnpackKind {
    let name = url.rsplit('/').next().unwrap_or(url);
    if name.ends_with(".zip") {
        UnpackKind::Zip
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        UnpackKind::TarGz
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tar.xz") {
        UnpackKind::Unsupported
    } else {
        UnpackKind::BareExecutable
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `None` means the registry published no hash — recorded as
/// [`Integrity::None`], never silently treated as verified.
pub(crate) fn verify_sha256(bytes: &[u8], expected: Option<&str>) -> Result<Integrity, ()> {
    match expected {
        None => Ok(Integrity::None),
        Some(expected) if sha256_hex(bytes).eq_ignore_ascii_case(expected) => Ok(Integrity::Sha256),
        Some(_) => Err(()),
    }
}

/// Rejects both traversal and absolute paths. Callers must additionally
/// reject non-regular, non-directory entries — a symlink pointing outside
/// the destination passes a path check and still escapes.
pub(crate) fn safe_entry_path(destination: &Path, entry: &Path) -> Result<PathBuf, ()> {
    if entry.is_absolute() {
        return Err(());
    }
    let mut resolved = destination.to_path_buf();
    for component in entry.components() {
        match component {
            std::path::Component::Normal(part) => resolved.push(part),
            std::path::Component::CurDir => {}
            _ => return Err(()),
        }
    }
    resolved.starts_with(destination).then_some(resolved).ok_or(())
}

pub(crate) fn staging_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(".staging").join(format!("{id}-{version}-{}", std::process::id()))
}

impl Installer {
    pub fn new(store: InstallStore) -> Self {
        Self { store }
    }

    /// Removes staging left by a process that is gone. Called once at
    /// startup: a killed install must not leak a directory — or a lock —
    /// forever.
    ///
    /// Lock files are swept unconditionally, because a lock that survived
    /// into a new run is stale by construction: the process that took it
    /// does not exist any more. (Two Tiller instances installing the same
    /// agent at the same moment is out of scope — the app is
    /// single-instance by the control-socket design — and the worst case if
    /// it ever happened is a redundant download into separate staging
    /// directories, not corruption, because the commit point is a
    /// single-file rename.)
    pub fn sweep_staging(&self) -> anyhow::Result<()> {
        let staging_root = self.store.root().join(".staging");
        let Ok(entries) = std::fs::read_dir(&staging_root) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.ends_with(".lock") {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let owner: Option<u32> = name.rsplit('-').next().and_then(|pid| pid.parse().ok());
            if owner == Some(std::process::id()) {
                continue;
            }
            let _ = std::fs::remove_dir_all(&path);
        }
        Ok(())
    }

    pub fn install(
        &self,
        agent: &RegistryAgent,
        platform_key: &str,
    ) -> Result<InstalledAgent, InstallError> {
        // Same precedence `resolve` uses, and for the same reason: prefer a
        // binary artifact built for this machine, fall back to npx only when
        // the agent publishes nothing for this platform. An agent may declare
        // both — `kilo` and `sigit` do.
        if let Some(artifact) = agent.distributions.iter().find_map(|distribution| match distribution
        {
            Distribution::Binary(artifacts) => artifacts.get(platform_key),
            _ => None,
        }) {
            return self.install_binary(agent, artifact);
        }
        // Task 6 replaces this arm with the real npx install.
        Err(InstallError::UnsupportedDistribution { agent: agent.id.clone() })
    }

    fn install_binary(
        &self,
        agent: &RegistryAgent,
        artifact: &BinaryArtifact,
    ) -> Result<InstalledAgent, InstallError> {
        let kind = unpack_kind(&artifact.archive);
        if kind == UnpackKind::Unsupported {
            return Err(InstallError::UnsupportedArchive {
                agent: agent.id.clone(),
                url: artifact.archive.clone(),
            });
        }

        let staging = staging_dir(self.store.root(), &agent.id, &agent.version);
        let _guard = InstallGuard::acquire(self.store.root(), &agent.id)?;
        let fail = |message: String| InstallError::Failed { agent: agent.id.clone(), message };

        std::fs::create_dir_all(&staging).map_err(|error| fail(error.to_string()))?;

        let bytes = download(&artifact.archive).map_err(|error| fail(error.to_string()))?;
        let integrity = verify_sha256(&bytes, artifact.sha256.as_deref())
            .map_err(|()| InstallError::ChecksumMismatch { agent: agent.id.clone() })?;

        match kind {
            UnpackKind::Zip => unpack_zip(&agent.id, &bytes, &staging)?,
            UnpackKind::TarGz => unpack_tar_gz(&agent.id, &bytes, &staging)?,
            UnpackKind::BareExecutable => {
                let name = artifact.cmd.trim_start_matches("./");
                std::fs::write(staging.join(name), &bytes)
                    .map_err(|error| fail(error.to_string()))?;
            }
            UnpackKind::Unsupported => unreachable!("rejected above"),
        }

        let relative = artifact.cmd.trim_start_matches("./");
        let command = staging.join(relative);
        if !command.exists() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::MissingCommand {
                agent: agent.id.clone(),
                cmd: artifact.cmd.clone(),
            });
        }
        set_executable(&command).map_err(|error| fail(error.to_string()))?;

        let final_dir = self.store.root().join(&agent.id).join(&agent.version);
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| fail(error.to_string()))?;
        }
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&staging, &final_dir).map_err(|error| fail(error.to_string()))?;

        let installed = InstalledAgent {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: final_dir.join(relative),
            args: artifact.args.clone(),
            integrity,
        };
        // The commit point: everything above is recoverable, this is not.
        self.store.write(&installed).map_err(|error| fail(error.to_string()))?;
        Ok(installed)
    }
}

/// A per-agent lock so a double-click on Install is a no-op rather than two
/// concurrent unpacks into the same path.
struct InstallGuard {
    path: PathBuf,
}

impl InstallGuard {
    fn acquire(root: &Path, id: &str) -> Result<Self, InstallError> {
        let path = root.join(".staging").join(format!("{id}.lock"));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| InstallError::AlreadyRunning { agent: id.to_string() })?;
        Ok(Self { path })
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    let mut body = ureq::get(url)
        .config()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build()
        .call()?
        .into_body()
        .into_reader()
        .take(MAX_DOWNLOAD_BYTES);
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn unpack_zip(agent: &str, bytes: &[u8], destination: &Path) -> Result<(), InstallError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| InstallError::Failed { agent: agent.into(), message: error.to_string() })?;
    let mut written: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let Some(name) = entry.enclosed_name() else {
            return Err(InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: entry.name().to_string(),
            });
        };
        let target = safe_entry_path(destination, &name).map_err(|()| {
            InstallError::UnsafeArchiveEntry { agent: agent.into(), entry: name.display().to_string() }
        })?;
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        written += entry.size();
        if written > MAX_UNPACKED_BYTES {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut file = std::fs::File::create(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        std::io::copy(&mut entry, &mut file).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
    }
    Ok(())
}

fn unpack_tar_gz(agent: &str, bytes: &[u8], destination: &Path) -> Result<(), InstallError> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().map_err(|error| InstallError::Failed {
        agent: agent.into(),
        message: error.to_string(),
    })?;
    let mut written: u64 = 0;
    for entry in entries {
        let mut entry = entry.map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        // A symlink or hardlink can point outside the destination and pass
        // a pure path check, so entry *type* is filtered before the path is
        // even considered.
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(InstallError::UnsafeArchiveEntry {
                agent: agent.into(),
                entry: format!("{kind:?}"),
            });
        }
        let path = entry.path().map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
        let target = safe_entry_path(destination, &path).map_err(|()| {
            InstallError::UnsafeArchiveEntry { agent: agent.into(), entry: path.display().to_string() }
        })?;
        if kind.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        written += entry.header().size().unwrap_or(0);
        if written > MAX_UNPACKED_BYTES {
            return Err(InstallError::Failed {
                agent: agent.into(),
                message: "unpacked size exceeds the ceiling".into(),
            });
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        entry.unpack(&target).map_err(|error| InstallError::Failed {
            agent: agent.into(),
            message: error.to_string(),
        })?;
    }
    Ok(())
}

/// Exactly one path, so `tar` (which preserves modes) and `zip` (which does
/// not) end up behaving the same and nothing extra is granted.
fn set_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
```

- [ ] **Step 5: Add `thiserror`**

```toml
thiserror = "2"
```

If `thiserror` is not in the local registry, implement `std::fmt::Display` and `std::error::Error` by hand rather than adding a network fetch — the error text in the tests is the requirement, the derive is not.

- [ ] **Step 6: Export from `lib.rs`**

Add `mod installer;` and `pub use installer::{InstallError, Installer, UnpackKind, unpack_kind};`.

- [ ] **Step 7: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 35 tests.

- [ ] **Step 8: Commit**

```bash
git add rust/crates/tiller_registry
git commit -m "feat(registry): install binary artifacts with verified unpacking"
```

---

### Task 6: Installing an npx package

**Files:**
- Modify: `rust/crates/tiller_registry/src/installer.rs`

**Interfaces:**
- Consumes: everything from Task 5.
- Produces: `resolve_bin_name(entries: &[String], package: &str) -> Option<String>`, and `Installer::install` now handling `Distribution::Npx`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `installer.rs`:

```rust
    #[test]
    fn a_single_bin_entry_wins() {
        let entries = vec!["claude-agent-acp".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "@agentclientprotocol/claude-agent-acp@0.70.0"),
            Some("claude-agent-acp".to_string())
        );
    }

    #[test]
    fn a_dependency_bin_is_never_preferred_over_the_package_bin() {
        // Installing codex-acp also drops a `codex` bin from its
        // @openai/codex dependency. Preferring it would silently launch the
        // wrong program — the Swift original's comment says "must never be
        // preferred".
        let entries = vec!["codex".to_string(), "codex-acp".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "@agentclientprotocol/codex-acp@1.6.2"),
            Some("codex-acp".to_string())
        );
    }

    #[test]
    fn the_longest_entry_contained_in_the_package_name_is_the_fallback() {
        let entries = vec!["ag".to_string(), "agent-cli".to_string()];
        assert_eq!(
            resolve_bin_name(&entries, "agent-cli-tools@1.0.0"),
            Some("agent-cli".to_string())
        );
    }

    #[test]
    fn no_bin_entries_yields_none_rather_than_a_guess() {
        assert_eq!(resolve_bin_name(&[], "whatever@1.0.0"), None);
    }

    #[test]
    fn dotfiles_are_not_candidates() {
        let entries = vec![".package-lock.json".to_string(), "real-bin".to_string()];
        assert_eq!(resolve_bin_name(&entries, "real-bin@1.0.0"), Some("real-bin".to_string()));
    }

    #[test]
    fn npm_runs_with_scripts_disabled_by_default() {
        let command = npm_install_argv("/tmp/staging", "codex-acp@1.6.2", false);
        assert!(
            command.contains(&"--ignore-scripts".to_string()),
            "installing must not run maintainer preinstall/postinstall by default"
        );
        let with_scripts = npm_install_argv("/tmp/staging", "codex-acp@1.6.2", true);
        assert!(!with_scripts.contains(&"--ignore-scripts".to_string()));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p tiller_registry installer
```

Expected: FAIL with "cannot find function `resolve_bin_name`".

- [ ] **Step 3: Implement the npx path**

Add to `installer.rs`:

```rust
/// Picks the launchable entry in `node_modules/.bin`.
///
/// A single entry wins. Otherwise the bin matching the package's own name
/// (scope and version stripped) wins — dependency bins land in the same
/// directory (`codex` beside `codex-acp` from `@openai/codex`) and must
/// never be preferred. Falls back to the longest entry contained in the
/// package name.
pub(crate) fn resolve_bin_name(entries: &[String], package: &str) -> Option<String> {
    let mut candidates: Vec<&String> =
        entries.iter().filter(|entry| !entry.starts_with('.')).collect();
    candidates.sort();
    if candidates.len() == 1 {
        return Some(candidates[0].clone());
    }
    let without_scope = package.rsplit('/').next().unwrap_or(package);
    let base = without_scope.split('@').next().filter(|name| !name.is_empty()).unwrap_or(without_scope);
    if let Some(exact) = candidates.iter().find(|entry| entry.as_str() == base) {
        return Some((*exact).clone());
    }
    candidates
        .iter()
        .filter(|entry| package.contains(entry.as_str()))
        .max_by_key(|entry| entry.len())
        .map(|entry| (*entry).clone())
        .or_else(|| candidates.first().map(|entry| (*entry).clone()))
}

/// The argv for one npm install. `--ignore-scripts` is the default: a
/// package's `preinstall`/`postinstall` runs maintainer code with the
/// user's privileges *before* they have chosen to launch that agent. The
/// `allow_scripts` path exists for the retry the UI offers after naming
/// which package asked for it.
pub(crate) fn npm_install_argv(prefix: &str, package: &str, allow_scripts: bool) -> Vec<String> {
    let mut argv = vec![
        "install".to_string(),
        "--prefix".to_string(),
        prefix.to_string(),
        "--no-audit".to_string(),
        "--no-fund".to_string(),
    ];
    if !allow_scripts {
        argv.push("--ignore-scripts".to_string());
    }
    argv.push(package.to_string());
    argv
}
```

Then replace the `Distribution::Npx` arm of `Installer::install`:

```rust
            Distribution::Npx { package, args } => self.install_npx(agent, package, args),
```

And add the method:

```rust
    fn install_npx(
        &self,
        agent: &RegistryAgent,
        package: &str,
        args: &[String],
    ) -> Result<InstalledAgent, InstallError> {
        let staging = staging_dir(self.store.root(), &agent.id, &agent.version);
        let _guard = InstallGuard::acquire(self.store.root(), &agent.id)?;
        let fail = |message: String| InstallError::Failed { agent: agent.id.clone(), message };

        std::fs::create_dir_all(&staging).map_err(|error| fail(error.to_string()))?;

        let output = std::process::Command::new(npm_binary())
            .args(npm_install_argv(&staging.to_string_lossy(), package, false))
            .current_dir(&staging)
            .output()
            .map_err(|error| {
                fail(format!(
                    "npm could not be run ({error}); the npx distribution needs Node on PATH"
                ))
            })?;
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(fail(String::from_utf8_lossy(&output.stderr).into_owned()));
        }

        let bin_dir = staging.join("node_modules/.bin");
        let entries: Vec<String> = std::fs::read_dir(&bin_dir)
            .map_err(|error| fail(error.to_string()))?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let bin = resolve_bin_name(&entries, package).ok_or_else(|| {
            fail("the installed package exposes no executable".to_string())
        })?;

        let final_dir = self.store.root().join(&agent.id).join(&agent.version);
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| fail(error.to_string()))?;
        }
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&staging, &final_dir).map_err(|error| fail(error.to_string()))?;

        let installed = InstalledAgent {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: final_dir.join("node_modules/.bin").join(&bin),
            args: args.to_vec(),
            // npm publishes no hash this code can check against the
            // registry document. Recorded honestly.
            integrity: Integrity::None,
        };
        self.store.write(&installed).map_err(|error| fail(error.to_string()))?;
        Ok(installed)
    }

fn npm_binary() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_registry
```

Expected: PASS, 41 tests.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_registry
git commit -m "feat(registry): install npx packages without running their scripts"
```

---

### Task 7: The in-binary claim, and the test that would have caught the bug

**Files:**
- Modify: `rust/crates/tiller_agents/src/lib.rs` (trait, ~line 181)
- Modify: `rust/crates/tiller_agents/src/opencode.rs` (~line 87)
- Modify: `rust/crates/tiller_agents/src/omp.rs` (~line 112)
- Create: `rust/crates/tiller_agents/tests/acp_conformance.rs`

**Interfaces:**
- Produces: `AgentAdapter::builtin_acp(&self) -> Option<AcpProgram>` with a `None` default. Task 8 consumes it. `acp_program` stays untouched in this task so the tree keeps building; Task 10 removes it.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `rust/crates/tiller_agents/src/lib.rs`:

```rust
    #[test]
    fn only_agents_that_serve_acp_from_their_own_binary_claim_it() {
        // The claim is deliberately narrow: it means "this CLI, already on
        // the user's machine, answers ACP on a subcommand". Claude, Codex
        // and Pi reach ACP through separate packages, so they claim
        // nothing here.
        assert_eq!(
            OpenCodeAdapter.builtin_acp(),
            Some(AcpProgram::new("opencode", &["acp"])),
            "verified live 2026-08-23: opencode 1.18.21 answers initialize"
        );
        for adapter in [
            &ClaudeCodeAdapter as &dyn AgentAdapter,
            &CodexAdapter,
            &PiAdapter,
        ] {
            assert_eq!(
                adapter.builtin_acp(),
                None,
                "{} reaches ACP through a package, not a subcommand",
                adapter.id()
            );
        }
    }

    #[test]
    fn oh_my_pi_claims_nothing_until_a_real_omp_is_verified() {
        // Ship gate, not a note. The retired Swift app launched `omp acp`
        // (AgentLaunchSpec.swift:73-75), but no omp has been available to
        // confirm it against, and asserting an unverified capability is the
        // exact defect this work exists to remove.
        assert_eq!(OhMyPiAdapter.builtin_acp(), None);
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd rust && cargo test -p tiller_agents builtin_acp
```

Expected: FAIL — "no method named `builtin_acp`".

- [ ] **Step 3: Add the trait method**

In `rust/crates/tiller_agents/src/lib.rs`, immediately after the existing `acp_program` declaration:

```rust
    /// The ACP server this adapter's own CLI serves, as a subcommand of a
    /// binary the user already installed — or `None`.
    ///
    /// Narrower than [`AgentAdapter::acp_program`] on purpose: this claim
    /// covers only what the local binary does, so it stays true without a
    /// network round-trip. Anything reachable through a separate package is
    /// the registry's business, not this trait's.
    ///
    /// The default is `None`: an adapter answers here only once its
    /// subcommand has been observed answering an ACP `initialize`.
    fn builtin_acp(&self) -> Option<AcpProgram> {
        None
    }
```

- [ ] **Step 4: Implement it for OpenCode**

Replace the body of `acp_program` in `rust/crates/tiller_agents/src/opencode.rs:87` — leave `acp_program` itself alone for now and add beside it:

```rust
    fn builtin_acp(&self) -> Option<crate::AcpProgram> {
        // Verified live on 2026-08-23 against opencode 1.18.21: piping an
        // ACP `initialize` into `opencode acp` returns
        // `agentInfo: {"name":"OpenCode","version":"1.18.21"}` with
        // sessionCapabilities close/fork/list/resume. The registry agrees —
        // its own `opencode` entry carries `cmd: "./opencode",
        // args: ["acp"]`.
        Some(crate::AcpProgram::new("opencode", &["acp"]))
    }
```

- [ ] **Step 5: Leave Oh-My-Pi claiming nothing, and say why**

In `rust/crates/tiller_agents/src/omp.rs`, add beside `acp_program`:

```rust
    fn builtin_acp(&self) -> Option<crate::AcpProgram> {
        // The retired Swift app launched `omp acp`
        // (AgentLaunchSpec.swift:73-75 at 5430d7bf), so this is very
        // probably `Some(AcpProgram::new("omp", &["acp"]))` — but no omp has
        // been installed anywhere this could be checked, and shipping an
        // unverified capability claim is precisely the defect this design
        // removes. Flip it when `tests/acp_conformance.rs` runs green
        // against a real omp instead of skipping.
        None
    }
```

- [ ] **Step 6: Write the gated conformance test**

`rust/crates/tiller_agents/tests/acp_conformance.rs`:

```rust
//! Does the CLI on this machine actually answer ACP?
//!
//! This is the test that would have caught the original defect: two
//! adapters denied a capability their CLI had, and nothing in the suite
//! could notice because nothing ever launched them. It SKIPs when the
//! binary is absent — in the style `Scripts/ci-linux.sh` already uses for
//! stages allowed to SKIP — so an unverified claim can never turn into a
//! false green.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use tiller_agents::{AgentAdapter, OhMyPiAdapter, OpenCodeAdapter};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false}}}}"#;

fn answers_initialize(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(format!("{INITIALIZE}\n").as_bytes()).ok()?;
    let stdout = child.stdout.take()?;
    let mut line = String::new();
    let read = BufReader::new(stdout).read_line(&mut line).ok()?;
    let _ = child.kill();
    (read > 0).then_some(line)
}

#[test]
fn opencode_answers_the_acp_handshake_it_claims() {
    let Some(program) = OpenCodeAdapter.availability().executable else {
        eprintln!("SKIP: opencode is not on PATH");
        return;
    };
    let claim = OpenCodeAdapter.builtin_acp().expect("opencode claims an in-binary ACP server");
    let response = answers_initialize(&program.to_string_lossy(), claim.args)
        .expect("opencode acp answered nothing on stdout");
    assert!(
        response.contains("\"protocolVersion\""),
        "expected an ACP initialize result, got: {response}"
    );
    assert!(response.contains("OpenCode"), "expected agentInfo naming OpenCode: {response}");
}

#[test]
fn oh_my_pi_is_only_claimed_once_it_answers() {
    let Some(program) = OhMyPiAdapter.availability().executable else {
        eprintln!("SKIP: omp is not on PATH — the claim stays None until it can be checked");
        assert_eq!(
            OhMyPiAdapter.builtin_acp(),
            None,
            "an unverifiable claim must not ship as Some"
        );
        return;
    };
    // omp is present: the ship gate can be settled either way, and the
    // adapter must agree with what the binary actually does.
    let answered = answers_initialize(&program.to_string_lossy(), &["acp"])
        .is_some_and(|response| response.contains("\"protocolVersion\""));
    assert_eq!(
        OhMyPiAdapter.builtin_acp().is_some(),
        answered,
        "the adapter's claim and the binary's behaviour must match"
    );
}
```

- [ ] **Step 7: Run the tests to verify they pass**

```bash
cd rust && cargo test -p tiller_agents
```

Expected: PASS. `opencode_answers_the_acp_handshake_it_claims` genuinely runs on a machine with opencode; `oh_my_pi_is_only_claimed_once_it_answers` prints SKIP and asserts the `None`.

- [ ] **Step 8: Commit**

```bash
git add rust/crates/tiller_agents
git commit -m "feat(agents): claim opencode's in-binary acp server and prove it"
```

---

### Task 8: Wire the resolution into the app

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Modify: `rust/crates/tiller/Cargo.toml`
- Modify: `rust/crates/tiller_ui/src/chat.rs:1022-1035` and `:1083-1096`
- Modify: `rust/crates/tiller_ui/Cargo.toml`

**Interfaces:**
- Consumes: `RegistryClient`, `InstallStore`, `Installer`, `resolve`, `ResolveInput`, `LaunchSource`, `current_platform_key` (Tasks 2-6); `AgentAdapter::builtin_acp` (Task 7).
- Produces: `AgentLaunchState` in `main.rs` — `{ registry: Option<AcpRegistry>, store: InstallStore, sources: BTreeMap<String, LaunchSource> }` — and `TillerWorkspace::launch_source_for(&self, adapter_id: &str) -> LaunchSource`. Task 9 renders from it.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `rust/crates/tiller/src/main.rs`:

```rust
    #[test]
    fn a_chat_launches_the_resolved_source_not_a_hardcoded_default() {
        // The two `npx …@latest` defaults in chat.rs were the last place a
        // chat could start a network fetch before it could say anything.
        let source = tiller_registry::LaunchSource::Builtin {
            program: "opencode".into(),
            args: vec!["acp".into()],
        };
        let command = agent_command_for(&source).expect("a builtin source launches");
        // `AgentCommand`'s fields are public (`tiller_acp/src/lib.rs:141-146`);
        // there are no accessor methods.
        assert_eq!(command.program, std::path::PathBuf::from("opencode"));
        assert_eq!(command.args, vec!["acp".to_string()]);
    }

    #[test]
    fn an_unavailable_source_launches_nothing() {
        let source = tiller_registry::LaunchSource::Unavailable(
            tiller_registry::UnavailableReason::NotInRegistry,
        );
        assert!(
            agent_command_for(&source).is_none(),
            "no fallback to another agent's server, and no invented program name"
        );
    }

    #[test]
    fn an_installable_source_launches_nothing_until_it_is_installed() {
        let agent = tiller_registry::RegistryAgent {
            id: "codex-acp".into(),
            name: "Codex".into(),
            version: "1.6.2".into(),
            description: None,
            repository: None,
            website: None,
            license: None,
            icon: None,
            distributions: vec![tiller_registry::Distribution::Npx {
                package: "@agentclientprotocol/codex-acp@1.6.2".into(),
                args: vec![],
            }],
        };
        assert!(agent_command_for(&tiller_registry::LaunchSource::Installable { agent }).is_none());
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd rust && cargo test -p tiller a_chat_launches_the_resolved_source
```

Expected: FAIL — "cannot find function `agent_command_for`".

- [ ] **Step 3: Add the dependency**

In `rust/crates/tiller/Cargo.toml` and `rust/crates/tiller_ui/Cargo.toml`:

```toml
tiller_registry = { workspace = true }
```

`tiller_ui` takes it for **types only** — no `RegistryClient`, `Installer` or filesystem call may appear there.

- [ ] **Step 4: Write the conversion in `main.rs`**

```rust
/// The `AgentCommand` a resolved source launches, or `None` when there is
/// nothing honest to launch. `Installable` deliberately returns `None`: an
/// agent that is not installed yet must offer an Install button, never a
/// silent download at chat-open time.
fn agent_command_for(source: &tiller_registry::LaunchSource) -> Option<AgentCommand> {
    match source {
        tiller_registry::LaunchSource::Builtin { program, args } => {
            Some(AgentCommand::new(program).args(args.iter().cloned()))
        }
        tiller_registry::LaunchSource::Installed(agent) => {
            Some(AgentCommand::new(&agent.executable).args(agent.args.iter().cloned()))
        }
        tiller_registry::LaunchSource::Installable { .. }
        | tiller_registry::LaunchSource::Unavailable(_) => None,
    }
}
```

- [ ] **Step 5: Hold the launch state on the workspace**

Add a field to `TillerWorkspace` and populate it at startup, off the UI thread:

```rust
/// Everything needed to answer "how does agent X launch". Refreshed when
/// Settings -> Agents opens and when Refresh is pressed; never on the UI
/// thread.
struct AgentLaunchState {
    registry: Option<tiller_registry::AcpRegistry>,
    store: tiller_registry::InstallStore,
    sources: std::collections::BTreeMap<String, tiller_registry::LaunchSource>,
}

impl TillerWorkspace {
    fn recompute_launch_sources(&mut self) {
        let platform = tiller_registry::current_platform_key();
        let mut sources = std::collections::BTreeMap::new();
        for adapter in tiller_agents::ALL {
            let availability = adapter.availability();
            let installed = tiller_registry::registry_id(adapter.id())
                .and_then(|id| self.launch.store.manifest(id))
                .map(|installed| {
                    let exists = installed.executable.exists();
                    (installed, exists)
                });
            let source = tiller_registry::resolve(tiller_registry::ResolveInput {
                adapter_id: adapter.id(),
                builtin: adapter.builtin_acp().map(|program| tiller_registry::BuiltinAcp {
                    program: program.program,
                    args: program.args,
                }),
                builtin_on_path: availability.is_available(),
                installed,
                registry: self.launch.registry.as_ref(),
                platform_key: platform,
            });
            sources.insert(adapter.id().to_string(), source);
        }
        self.launch.sources = sources;
    }

    fn launch_source_for(&self, adapter_id: &str) -> tiller_registry::LaunchSource {
        self.launch
            .sources
            .get(adapter_id)
            .cloned()
            .unwrap_or(tiller_registry::LaunchSource::Unavailable(
                tiller_registry::UnavailableReason::NotInRegistry,
            ))
    }
}
```

At startup, before the first render, run `Installer::new(store).sweep_staging()` and the first `RegistryClient::registry(DEFAULT_MAX_AGE, false)` on `cx.background_executor()`, then `recompute_launch_sources` on the foreground.

- [ ] **Step 6: Replace both chat-tab launch paths**

There are **two** sites resolving `adapter.acp_program()` into `acp_agent_command`, not one — `main.rs:2720-2724` and `main.rs:7186-7193` (inside `add_chat_tab`). Convert both to `self.launch_source_for(adapter_id)` plus `agent_command_for(&source)`. Leaving either behind keeps one launch path on the compile-time claim, and Task 10's removal would then fail to compile.

When `agent_command_for` returns `None`, do not open a chat tab — surface the reason carried by `LaunchSource` rather than opening a tab that cannot connect.

- [ ] **Step 7: Delete the two hardcoded defaults in `chat.rs`**

`rust/crates/tiller_ui/src/chat.rs:1022-1035` — `Chat::launch` currently falls back to `npx -y @agentclientprotocol/claude-agent-acp@latest`. Change it to require a command:

```rust
    /// Launches a real ACP agent from the command the caller resolved.
    ///
    /// There is deliberately no default: a hardcoded `npx …@latest` here
    /// meant every chat tab could start a network fetch before it could say
    /// anything, and silently connected a tab to Claude's server whatever
    /// agent the user picked. `TILLER_ACP_PROGRAM` stays as a test escape
    /// hatch, because integration tests need one.
    pub fn launch_from_env(cx: &mut Context<Self>) -> Option<Self> {
        let command = std::env::var_os("TILLER_ACP_PROGRAM")
            .map(PathBuf::from)
            .map(AgentCommand::new)?;
        Some(Self::launch_with_command(command, default_agent_cwd(), cx))
    }
```

Apply the same removal at `:1083-1096`. Update every caller of `Chat::launch`.

- [ ] **Step 8: Run the tests**

```bash
cd rust && cargo test -p tiller -p tiller_ui
```

Expected: PASS. Fix any caller the signature change broke.

- [ ] **Step 9: Commit**

```bash
git add rust/crates/tiller rust/crates/tiller_ui
git commit -m "feat(chat): launch the resolved agent instead of a hardcoded npx default"
```

---

### Task 9: The Agents screen tells the truth

**Files:**
- Modify: `rust/crates/tiller_ui/src/settings.rs:3161-3189` (`render_acp_badge`)
- Modify: `rust/crates/tiller_ui/src/settings.rs:3190+` (`render_agents`)
- Modify: `rust/crates/tiller_ui/src/settings.rs:638-660` (`provider_row`, `ProviderRowModel`) — and its four call sites: `:3199`, `:4436`, `:4451`, `:6997`. The last three are tests; an unlisted call site is how a task reports green while the crate does not build.
- Modify: `rust/crates/tiller_ui/src/tab_bar.rs:520` — the new-chat picker filters on `agent.acp_program().is_some()`. It is the only consumer of that method outside Settings, and Task 10 removes it, so the picker moves to `LaunchSource` here, where the same state is already being threaded.
- Modify: `rust/crates/tiller_ui/src/settings.rs:4942-5010` (the existing badge test)
- Modify: `rust/crates/tiller/src/main.rs` — add `bind_settings`, modelled on `bind_chat` (`main.rs:4442`), to receive `SettingsEvent` and run the install on the background executor

**Interfaces:**
- Consumes: `LaunchSource`, `UnavailableReason`, `Integrity` (Task 2).
- Produces: `launch_badge_label(&LaunchSource) -> &'static str`, `installed_integrity_note(&LaunchSource) -> Option<&'static str>`, `Settings::with_launch_sources(Vec<(String, LaunchSource)>) -> Self` (mirrors the existing `with_availability` builder, `settings.rs:1373`), and `SettingsEvent { InstallAgent(String), UpdateAgent(String) }`.

**How the screen tells the host to install something.** `Settings` has no outward channel today: there is no `EventEmitter`, no `cx.emit`, and `SettingsReport` (`settings.rs:582-597`) is a state snapshot the host *reads*, not a place to put a request. Do **not** invent a parallel mechanism. `Chat` already solves exactly this problem — `impl EventEmitter<ChatEvent> for Chat`, with the host subscribing in `bind_chat` (`main.rs:4442`) — so add `impl EventEmitter<SettingsEvent> for Settings` and a matching `bind_settings` in `main.rs`. The button emits; `tiller` owns the install.

- [ ] **Step 1: Write the failing tests**

Replace `agent_rows_mark_adapters_without_an_acp_server` in `settings.rs` with:

```rust
    #[test]
    fn the_badge_reports_the_resolved_source_rather_than_a_compiled_claim() {
        use tiller_registry::{LaunchSource, UnavailableReason};

        assert_eq!(
            launch_badge_label(&LaunchSource::Builtin {
                program: "opencode".into(),
                args: vec!["acp".into()],
            }),
            "ACP chat available",
            "OpenCode serves ACP from its own binary; the old pill said the opposite"
        );
        assert_eq!(
            launch_badge_label(&LaunchSource::Unavailable(UnavailableReason::NotInRegistry)),
            "No ACP server",
        );
        assert_eq!(
            launch_badge_label(&LaunchSource::Unavailable(
                UnavailableReason::NoArtifactForPlatform
            )),
            "Not available for this platform",
            "an undifferentiated grey pill is the failure mode this work removes"
        );
        assert_eq!(
            launch_badge_label(&LaunchSource::Unavailable(
                UnavailableReason::UnsupportedDistribution
            )),
            "Unsupported install format",
        );
    }

    #[test]
    fn an_installable_row_offers_install_and_says_when_it_cannot_be_verified() {
        use tiller_registry::{Distribution, LaunchSource, RegistryAgent};

        let agent = RegistryAgent {
            id: "cursor".into(),
            name: "Cursor".into(),
            version: "1.0.0".into(),
            description: None,
            repository: None,
            website: None,
            license: None,
            icon: None,
            distributions: vec![Distribution::Binary(Default::default())],
        };
        let source = LaunchSource::Installable { agent };
        assert_eq!(launch_badge_label(&source), "Install");
    }

    #[test]
    fn an_unverified_install_says_so_after_the_fact() {
        use tiller_registry::{InstalledAgent, Integrity, LaunchSource};

        let installed = InstalledAgent {
            id: "cursor".into(),
            version: "1.0.0".into(),
            executable: "/data/cursor".into(),
            args: vec![],
            integrity: Integrity::None,
        };
        assert_eq!(
            installed_integrity_note(&LaunchSource::Installed(installed)),
            Some("no published checksum"),
            "9 of 18 binary agents publish at least one unhashed artifact; that stays visible"
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd rust && cargo test -p tiller_ui launch_badge
```

Expected: FAIL — "cannot find function `launch_badge_label`".

- [ ] **Step 3: Implement the label and note**

```rust
/// The pill for one agent row. Every arm is a rendering of a resolved fact:
/// the previous version asked `acp_program()`, a compile-time claim, which
/// is why OpenCode and Oh-My-Pi were labelled "No ACP server" while their
/// binaries served one.
pub(crate) fn launch_badge_label(source: &tiller_registry::LaunchSource) -> &'static str {
    use tiller_registry::{LaunchSource, UnavailableReason};
    match source {
        LaunchSource::Builtin { .. } | LaunchSource::Installed(_) => "ACP chat available",
        LaunchSource::Installable { .. } => "Install",
        LaunchSource::Unavailable(UnavailableReason::NoArtifactForPlatform) => {
            "Not available for this platform"
        }
        LaunchSource::Unavailable(UnavailableReason::UnsupportedDistribution) => {
            "Unsupported install format"
        }
        LaunchSource::Unavailable(UnavailableReason::NotInRegistry) => "No ACP server",
    }
}

/// What an installed agent could not prove about itself, or `None` when it
/// could.
pub(crate) fn installed_integrity_note(
    source: &tiller_registry::LaunchSource,
) -> Option<&'static str> {
    match source {
        tiller_registry::LaunchSource::Installed(agent)
            if agent.integrity == tiller_registry::Integrity::None =>
        {
            Some("no published checksum")
        }
        _ => None,
    }
}
```

- [ ] **Step 4: Render the version and the action**

In `render_acp_badge`, take `&LaunchSource` instead of `&AgentAvailability`, use `launch_badge_label`, and keep the existing colour rule: the neutral raised pill (`theme.primary_pill_bg`) for `Builtin`/`Installed`, the warning tone (`theme.tab_needs_input`) for everything else.

`provider_row` (`settings.rs:635-660`) builds the `ProviderRowModel` from an `AgentAvailability`; give it the row's `LaunchSource` too so the badge and the action come from the same model rather than two sources that can disagree.

In `render_agents`, add per row: the version string (`Installed`/`Installable` carry one) and a button emitting `SettingsEvent::InstallAgent(id)` for `Installable`, or `SettingsEvent::UpdateAgent(id)` when an `Installed` version is behind the registry's. This closes `F-SET-18` (`docs/linux-rewrite/triage/T3-set.md:20`), which was half-proven precisely because no Install/Update control was drawn.

Buttons emit; they never call the installer directly — `tiller` owns that (Task 8), and `tiller_ui` has no filesystem or network access in this design.

- [ ] **Step 5: Run the tests**

```bash
cd rust && cargo test -p tiller_ui
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_ui
git commit -m "feat(settings): render the resolved launch source with install actions"
```

---

### Task 10: Delete the compile-time claim

**Files:**
- Modify: `rust/crates/tiller_agents/src/lib.rs` (remove `acp_program` from the trait and from `AgentAvailability`; keep `AcpProgram`)
- Modify: `rust/crates/tiller_agents/src/{claude,codex,opencode,pi,omp}.rs`
- Modify: `rust/crates/tiller_ui/src/chat.rs` (`acp_agent_command`, ~line 897; its doc reference at `:1040-1042`; its test at `:10317`)
- Modify: `rust/crates/tiller_ui/src/tab_bar.rs:520` (converted by Task 9; confirm no `acp_program` reference survives)
- Modify: `rust/crates/tiller_acp/src/lib.rs:2745`, `rust/crates/tiller_acp/src/mcp_config.rs:17`, `rust/crates/tiller_acp/tests/real_claude.rs`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Find every remaining literal**

```bash
cd rust && grep -rn "claude-agent-acp\|codex-acp" crates --include=*.rs
```

Expected before this task: 11 occurrences of `claude-agent-acp` across 8 files, 6 of `codex-acp` across 5. Every one that names a launch path must go; the ones in test fixtures that stand in for "some ACP program" may stay if they no longer drive a real launch.

- [ ] **Step 2: Write the failing test**

Add to `rust/crates/tiller_agents/src/lib.rs`:

```rust
    #[test]
    fn no_adapter_names_a_package_it_would_have_to_download() {
        // The launch path must not contain a package name at all: pinning
        // and installing are the registry's job, and a literal here is how
        // the previous version ended up fetching from the network before a
        // chat could say anything.
        for adapter in ALL {
            if let Some(program) = adapter.builtin_acp() {
                assert_ne!(program.program, "npx", "{} still launches npx", adapter.id());
                assert!(
                    !program.args.iter().any(|arg| arg.contains('@')),
                    "{} still names a package version",
                    adapter.id()
                );
            }
        }
    }
```

- [ ] **Step 3: Remove `acp_program`**

Delete the trait method (`lib.rs:181`), `AgentAvailability::acp_program` (`:51`), `AgentAvailability::acp_status_label` (`:60`), and the five adapter implementations. Delete the two now-obsolete tests `acp_program_differs_by_adapter_with_an_honest_none` and `availability_surface_reports_acp_support_without_a_fallback` — their replacements landed in Tasks 7 and 9.

Keep `AcpProgram`: `builtin_acp` returns it.

- [ ] **Step 4: Remove `acp_agent_command`**

`tiller_ui/src/chat.rs:897` exists only to convert an `AcpProgram` into an `AgentCommand` for the old path. Task 8's `agent_command_for` replaces it. Delete it and its test `acp_program_converts_to_the_adapters_own_agent_command` (`chat.rs:10317`).

- [ ] **Step 5: Update the crate diagram in CLAUDE.md**

In the "Crate boundaries" block, add `tiller_registry` to the leaf row and to `tiller_ui`/`tiller`'s dependency lists:

```
tiller_theme, tiller_project, tiller_git, tiller_persistence,
tiller_agents, tiller_activity, tiller_markdown, tiller_usage,
tiller_registry   (leaves — no local deps)
```

and extend the `tiller_ui` line with `tiller_registry`. Add one sentence under "Agent adapters" recording that an adapter's ACP claim now covers only its own binary's subcommand, and that everything else resolves through `tiller_registry`.

- [ ] **Step 6: Run the whole gate**

```bash
cd rust && cargo build --workspace && cargo test --workspace
bash Scripts/ci.sh
```

Expected: `CI OK`. Per CLAUDE.md, if a workspace-wide run flakes, re-run the affected crate alone with `cargo test -p <crate>` before treating it as a real failure.

- [ ] **Step 7: Verify the defect is gone, live**

```bash
cd rust && cargo test -p tiller_agents --test acp_conformance -- --nocapture
```

Expected: `opencode_answers_the_acp_handshake_it_claims` passes against the real binary. That, not a unit test, is the proof the original screenshot's pill was wrong and now is not.

- [ ] **Step 8: Commit**

```bash
git add rust CLAUDE.md
git commit -m "refactor(agents): drop the compile-time acp claim and its npx literals"
```

---

## Self-Review

**Spec coverage.** Every section of the design has a task: crate boundary and `resolve` signature (Tasks 1-2), `model`/`client`/`store`/`installer` (Tasks 1, 3, 4, 5-6), the identity join (Task 2), the ladder and its truth table (Task 2), update policy actions (Task 9), error handling (Tasks 3-6), UI (Task 9), testing (throughout), the security model's `--ignore-scripts` decision (Task 6) and unhashed-artifact disclosure (Tasks 5 and 9), the `omp` ship gate (Task 7), the `CLAUDE.md` diagram (Task 10).

**Deliberately not implemented**, each named in the spec or in Global Constraints rather than left silent: `uvx`, `.tar.bz2`, the update *cadence* wiring beyond the actions themselves (Task 9 draws the controls; scheduling them against the 24h cache is one line in Task 8's startup path), and the three native protocol drivers.

**Known gap to watch during execution.** Task 8 changes `Chat::launch`'s signature, which ripples through callers this plan cannot enumerate without reading them at execution time. Budget for that in Task 8 Step 8 rather than treating a broken caller as a surprise.

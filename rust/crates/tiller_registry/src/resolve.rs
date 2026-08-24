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

/// Whether the id names one of Tiller's five built-in adapters.
fn is_known_adapter(adapter_id: &str) -> bool {
    matches!(adapter_id, "claude" | "codex" | "pi" | "opencode" | "omp")
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

    // A known adapter goes through the explicit table, and a table `None`
    // is final: if the registry ever published an agent literally named
    // "omp", it must not silently start answering the omp lookup. Any other
    // id is a bare registry agent and uses its own id.
    let lookup = if is_known_adapter(input.adapter_id) {
        registry_id(input.adapter_id)
    } else {
        Some(input.adapter_id)
    };
    let Some(agent) = lookup.and_then(|id| registry.agent(id)) else {
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
        return LaunchSource::Installable {
            agent: agent.clone(),
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn builtin() -> BuiltinAcp {
        BuiltinAcp {
            program: "opencode",
            args: &["acp"],
        }
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
            LaunchSource::Builtin {
                program: "opencode".into(),
                args: vec!["acp".into()]
            },
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
        assert_eq!(
            registry_id("omp"),
            None,
            "omp is not in the registry at all"
        );
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

    #[test]
    fn npx_covers_a_platform_the_binary_does_not_build_for() {
        // The kilo/sigit shape: a binary that exists only for other
        // platforms, plus npx which works wherever Node does. Regression
        // here breaks those agents on exactly the platforms they do not
        // build for.
        let registry = registry_with(
            "kilo-like",
            vec![
                binary_for("darwin-aarch64"),
                Distribution::Npx {
                    package: "@example/kilo-acp".into(),
                    args: Vec::new(),
                },
            ],
        );
        let source = resolve(ResolveInput {
            adapter_id: "kilo-like",
            ..input(Some(&registry))
        });
        match source {
            LaunchSource::Installable { agent } => assert_eq!(agent.id, "kilo-like"),
            other => panic!("expected Installable via npx, got {other:?}"),
        }
    }

    #[test]
    fn an_installed_agent_beats_an_installable_registry_row() {
        // Both rungs in competition: the managed copy wins over downloading.
        let registry = registry_with("opencode", vec![binary_for("linux-x86_64")]);
        assert_eq!(
            resolve(ResolveInput {
                installed: Some((installed(), true)),
                ..input(Some(&registry))
            }),
            LaunchSource::Installed(installed())
        );
    }

    #[test]
    fn nothing_known_at_all_is_not_in_registry() {
        // Reaches the `input.registry` guard: no builtin, no install, and
        // no cached registry copy.
        assert_eq!(
            resolve(input(None)),
            LaunchSource::Unavailable(UnavailableReason::NotInRegistry)
        );
    }

    #[test]
    fn a_known_adapter_never_falls_through_to_bare_name_lookup() {
        // If the registry ever published an agent literally called "omp",
        // it must not start answering the omp adapter's lookup: the table's
        // `None` means "omp is reachable only through its own subcommand",
        // and that answer is final.
        let registry = registry_with("omp", vec![binary_for("linux-x86_64")]);
        assert_eq!(
            resolve(ResolveInput {
                adapter_id: "omp",
                ..input(Some(&registry))
            }),
            LaunchSource::Unavailable(UnavailableReason::NotInRegistry)
        );
    }
}

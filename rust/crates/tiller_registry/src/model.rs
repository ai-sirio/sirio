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
    /// Every kind the document declares for this agent, in document order.
    /// Most agents declare one; a few declare two (binary + npx). Choosing
    /// between them belongs to whoever knows the platform, not to decoding.
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
    /// Raw on purpose: decoded kind by kind below, so one malformed
    /// artifact costs one platform, not the whole agent or document.
    #[serde(default)]
    distribution: serde_json::Value,
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
                let distribution =
                    decode_distribution(&agent.id, agent.distribution, &mut warnings);
                RegistryAgent {
                    id: agent.id,
                    name: agent.name,
                    version: agent.version,
                    description: agent.description,
                    repository: agent.repository,
                    website: agent.website,
                    license: agent.license,
                    icon: agent.icon,
                    distributions: distribution,
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
    wire: serde_json::Value,
    warnings: &mut Vec<String>,
) -> Vec<Distribution> {
    // Walked in map order, which is the document's order for every agent
    // published today (`binary` < `npx` < `uvx`). A kind this build has
    // never heard of is skipped here and disclosed as [`Distribution::Unknown`]
    // when nothing else decoded.
    let entries: Vec<(String, serde_json::Value)> = match wire {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for (kind, value) in entries {
        match kind.as_str() {
            "npx" => {
                if let Ok(package) = serde_json::from_value::<PackageWire>(value) {
                    out.push(Distribution::Npx { package: package.package, args: package.args });
                }
            }
            "binary" => {
                let serde_json::Value::Object(platforms) = value else { continue };
                let artifacts: BTreeMap<String, BinaryArtifact> = platforms
                    .into_iter()
                    .filter_map(|(platform, artifact)| {
                        match serde_json::from_value::<ArtifactWire>(artifact) {
                            Ok(artifact) if artifact.archive.starts_with("https://") => Some((
                                platform,
                                BinaryArtifact {
                                    archive: artifact.archive,
                                    cmd: artifact.cmd,
                                    args: artifact.args,
                                    sha256: artifact.sha256,
                                },
                            )),
                            // Rejected before anything can fetch it: an
                            // `http://` or `file://` archive is a downgrade,
                            // not a variation.
                            Ok(_) => {
                                warnings.push(format!(
                                    "{agent_id}: dropped {platform} artifact with non-https archive URL"
                                ));
                                None
                            }
                            Err(_) => {
                                warnings.push(format!(
                                    "{agent_id}: dropped {platform} artifact missing required fields"
                                ));
                                None
                            }
                        }
                    })
                    .collect();
                out.push(Distribution::Binary(artifacts));
            }
            "uvx" => {
                if let Ok(package) = serde_json::from_value::<PackageWire>(value) {
                    out.push(Distribution::Uvx { package: package.package, args: package.args });
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        vec![Distribution::Unknown]
    } else {
        out
    }
}

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
        let artifacts = opencode
            .distributions
            .iter()
            .find_map(|dist| match dist {
                Distribution::Binary(artifacts) => Some(artifacts),
                _ => None,
            })
            .expect("opencode declares a binary distribution");
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
            agent.distributions.iter().any(|dist| match dist {
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
        let Distribution::Binary(artifacts) = &agent.distributions[0] else {
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

    #[test]
    fn a_malformed_artifact_costs_one_platform_not_the_document() {
        // An artifact missing `cmd` must not fail the whole map, the whole
        // agent, or the whole document — exactly one platform is lost.
        let registry = AcpRegistry::from_json(include_str!(
            "../tests/fixtures/registry-malformed-artifact.json"
        ))
        .expect("one broken artifact must not sink the document");

        let agent = registry.agent("broken-artifact-agent").expect("row survives");
        let Distribution::Binary(artifacts) = &agent.distributions[0] else {
            panic!("expected a binary distribution");
        };
        assert!(artifacts.contains_key("linux-x86_64"), "the valid platform stays");
        assert!(
            !artifacts.contains_key("darwin-aarch64"),
            "the artifact missing `cmd` is dropped"
        );
        assert_eq!(registry.warnings.len(), 1);
        assert!(registry.warnings[0].contains("broken-artifact-agent"));
    }

    #[test]
    fn multi_kind_agents_keep_every_kind_in_document_order() {
        // kilo and sigit declare binary AND npx. Both survive, in the order
        // the document lists them; choosing between them belongs to whoever
        // knows the platform, in a later task.
        let registry =
            AcpRegistry::from_json(include_str!("../tests/fixtures/registry-v1.json")).unwrap();

        for (id, platforms) in [("kilo", 5), ("sigit", 6)] {
            let agent = registry.agent(id).unwrap_or_else(|| panic!("{id} row"));
            assert_eq!(agent.distributions.len(), 2, "{id} keeps both kinds");
            let Distribution::Binary(artifacts) = &agent.distributions[0] else {
                panic!("{id}: first kind is binary, got {:?}", agent.distributions[0]);
            };
            assert_eq!(artifacts.len(), platforms, "{id} binary platforms");
            assert!(matches!(&agent.distributions[1], Distribution::Npx { .. }), "{id} second kind is npx");
        }
    }
}

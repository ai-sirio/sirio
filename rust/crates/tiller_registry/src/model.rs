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
    pub distribution: Distribution,
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
        let Distribution::Binary(artifacts) = &opencode.distribution else {
            panic!("opencode is a binary distribution, got {:?}", opencode.distribution);
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
        let unhashed = registry.agents.iter().any(|agent| match &agent.distribution {
            Distribution::Binary(artifacts) => {
                artifacts.values().any(|artifact| artifact.sha256.is_none())
            }
            _ => false,
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
        assert_eq!(agent.distribution, Distribution::Unknown);
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

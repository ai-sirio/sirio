//! Where a `sirio_lsp::Recipe` becomes something `sirio_registry` installs.
//!
//! The only file that sees both vocabularies, which is what lets the two
//! crates stay leaves. `sirio_lsp` never learns what a `RegistryAgent` is;
//! `sirio_registry` never learns what a language is.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sirio_lsp::Recipe;
use sirio_registry::{BinaryArtifact, Distribution, RegistryAgent};

/// Installed servers live beside installed agents, never among them: the
/// store is keyed by id, and `clangd` must not be able to collide with an
/// agent of the same name.
pub fn store_root(environment: &BTreeMap<String, String>) -> PathBuf {
    sirio_registry::InstallStore::default_root(environment).with_file_name("language-servers")
}

/// The installer's shape for this recipe on this platform, or `None` when
/// there is nothing to install — a `Manual` recipe, or a platform the
/// project publishes nothing for.
pub fn agent_for(recipe: &Recipe, platform: &str) -> Option<RegistryAgent> {
    let (id, version, distribution) = match recipe {
        Recipe::Manual { .. } => return None,
        Recipe::Npm {
            package, version, ..
        } => (
            *package,
            *version,
            Distribution::Npx {
                // The version rides in the package spec, which is what
                // `npm install` already accepts and what npm_install_argv
                // passes through untouched.
                package: format!("{package}@{version}"),
                args: Vec::new(),
            },
        ),
        Recipe::Release {
            id,
            version,
            assets,
        } => {
            let asset = assets
                .iter()
                .find(|(key, _)| *key == platform)
                .map(|(_, asset)| asset)?;
            let mut artifacts = BTreeMap::new();
            artifacts.insert(
                platform.to_string(),
                BinaryArtifact {
                    archive: asset.url.to_string(),
                    // Per asset, not per recipe: lemminx's archives are
                    // platform-named and the Windows ones carry a `.exe`.
                    cmd: asset.bin.to_string(),
                    args: Vec::new(),
                    sha256: Some(asset.sha256.to_string()),
                },
            );
            (*id, *version, Distribution::Binary(artifacts))
        }
    };
    Some(RegistryAgent {
        id: id.to_string(),
        name: id.to_string(),
        version: version.to_string(),
        description: None,
        repository: None,
        website: None,
        license: None,
        icon: None,
        distributions: vec![distribution],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_recipe_names_an_archive_the_installer_cannot_unpack() {
        // This test exists only because of the crate boundary. sirio_lsp
        // cannot see it — it does not know UnpackKind, and must not; they are
        // two leaves. sirio_registry cannot see it — it does not know what a
        // recipe is. Here, the boundary stops being a cost and becomes the
        // vantage point.
        //
        // It is also the test that would have caught the .gz bug before a line
        // of install code was written.
        for entry in sirio_lsp::LanguageTable::defaults().entries() {
            let Some(sirio_lsp::Recipe::Release { assets, .. }) = &entry.install else {
                continue;
            };
            for (platform, asset) in *assets {
                assert_ne!(
                    sirio_registry::unpack_kind(asset.url),
                    sirio_registry::UnpackKind::Unsupported,
                    "`{}` on {platform} names an archive the installer cannot unpack: {}",
                    entry.name,
                    asset.url
                );
            }
        }
    }

    #[test]
    fn every_release_asset_carries_a_full_sha256() {
        for entry in sirio_lsp::LanguageTable::defaults().entries() {
            let Some(sirio_lsp::Recipe::Release { assets, .. }) = &entry.install else {
                continue;
            };
            for (platform, asset) in *assets {
                assert_eq!(
                    asset.sha256.len(),
                    64,
                    "`{}` on {platform} has no usable hash",
                    entry.name
                );
                assert!(
                    asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                    "`{}` on {platform} has a hash that is not hex",
                    entry.name
                );
                assert!(
                    asset.bytes > 0,
                    "`{}` on {platform} claims no size",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn a_release_recipe_becomes_a_binary_distribution_for_this_platform() {
        let recipe = sirio_lsp::Recipe::Release {
            id: "clangd",
            version: "22.1.6",
            assets: &[(
                "linux-x86_64",
                sirio_lsp::Asset {
                    url: "https://example.invalid/clangd-linux-22.1.6.zip",
                    sha256: "a9c77443af2e447ed467e84771848d3a6ac1c56f84bcfcde717e66318de77cfa",
                    bytes: 114_790_601,
                    bin: "clangd_22.1.6/bin/clangd",
                },
            )],
        };
        let agent = agent_for(&recipe, "linux-x86_64").expect("this platform is published");
        assert_eq!(agent.id, "clangd");
        assert_eq!(agent.version, "22.1.6");
        match &agent.distributions[..] {
            [sirio_registry::Distribution::Binary(artifacts)] => {
                let artifact = artifacts.get("linux-x86_64").expect("the artifact");
                assert_eq!(artifact.cmd, "clangd_22.1.6/bin/clangd");
                assert_eq!(
                    artifact.sha256.as_deref(),
                    Some("a9c77443af2e447ed467e84771848d3a6ac1c56f84bcfcde717e66318de77cfa"),
                    "the hash reaches the installer, so Integrity::None never happens here"
                );
            }
            other => panic!("expected one binary distribution, got {other:?}"),
        }
    }

    #[test]
    fn a_platform_the_project_does_not_publish_for_resolves_to_nothing() {
        let recipe = sirio_lsp::Recipe::Release {
            id: "marksman",
            version: "2026.1.1",
            assets: &[(
                "linux-x86_64",
                sirio_lsp::Asset {
                    url: "https://example.invalid/marksman-linux-x64",
                    // 64 zeros written out: `"0".repeat(64).leak()` is not
                    // const-evaluable, and the array behind `&'static [..]`
                    // is only promoted to `'static` when every element is.
                    sha256: "0000000000000000000000000000000000000000000000000000000000000000",
                    bytes: 1,
                    bin: "marksman",
                },
            )],
        };
        assert!(agent_for(&recipe, "windows-aarch64").is_none());
    }

    #[test]
    fn a_manual_recipe_is_never_handed_to_the_installer() {
        let recipe = sirio_lsp::Recipe::Manual {
            needs: "a JVM",
            url: "https://example.invalid/",
        };
        assert!(agent_for(&recipe, "linux-x86_64").is_none());
    }

    #[test]
    fn the_language_server_store_is_a_sibling_of_the_agent_store() {
        let mut environment = std::collections::BTreeMap::new();
        environment.insert("XDG_DATA_HOME".to_string(), "/data".to_string());
        assert_eq!(
            store_root(&environment),
            std::path::PathBuf::from("/data/sirio/language-servers")
        );
        assert_ne!(
            store_root(&environment),
            sirio_registry::InstallStore::default_root(&environment),
            "servers and agents must not share a directory: the ids would collide"
        );
    }
}

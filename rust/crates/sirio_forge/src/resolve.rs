//! Which forge a host runs, and how Sirio talks to it (spec §5.2).
//!
//! The first step that answers wins: the user's setting, a known public
//! name, a signed-in CLI, a stored token, and only then the unauthenticated
//! GitHub Enterprise probe. A forge that none of them identifies is not
//! guessed: the UI asks the user, and the answer becomes a setting.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Forge;
use crate::transport::{CliOutput, CliProgram, http_agent, run, web_base};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Means {
    Cli,
    Token,
}

/// A host the user configured in Settings → Git hosting. `means: None`
/// fixes only the forge and leaves the means to detection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSetting {
    pub host: String,
    pub forge: Forge,
    #[serde(default)]
    pub means: Option<Means>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Talk to `forge` by `means`. A means that later fails is reported, not
    /// swapped for the other one (spec §5.2, "no silent fallback").
    Ready { forge: Forge, means: Means },
    /// The forge is known, but there is no signed-in CLI and no token.
    NotConnected { forge: Forge },
    /// Nothing identifies the forge; the user must say which it is.
    UnknownForge,
}

/// What detection may ask the system. [`SystemProbes`] asks the real one.
pub trait Probes {
    /// This forge's CLI is installed and signed in to `host`.
    fn cli_signed_in(&self, forge: Forge, host: &str) -> bool;
    /// `host` answers `/api/v3/meta` the way GitHub Enterprise Server does.
    fn is_github_enterprise(&self, host: &str) -> bool;
}

/// The two public forges, recognised by name.
pub fn known_forge(host: &str) -> Option<Forge> {
    match host {
        "github.com" => Some(Forge::GitHub),
        "gitlab.com" => Some(Forge::GitLab),
        _ => None,
    }
}

pub fn resolve(
    host: &str,
    setting: Option<&HostSetting>,
    stored_token: Option<Forge>,
    probes: &dyn Probes,
) -> Resolution {
    if let Some(setting) = setting
        && let Some(means) = setting.means
    {
        return match (means, stored_token) {
            (Means::Token, None) => Resolution::NotConnected {
                forge: setting.forge,
            },
            _ => Resolution::Ready {
                forge: setting.forge,
                means,
            },
        };
    }
    let known = setting
        .map(|setting| setting.forge)
        .or_else(|| known_forge(host));
    let candidates: &[Forge] = match known {
        Some(Forge::GitHub) => &[Forge::GitHub],
        Some(Forge::GitLab) => &[Forge::GitLab],
        None => &[Forge::GitHub, Forge::GitLab],
    };
    for &forge in candidates {
        if probes.cli_signed_in(forge, host) {
            return Resolution::Ready {
                forge,
                means: Means::Cli,
            };
        }
    }
    if let Some(token_forge) = stored_token
        && known.is_none_or(|forge| forge == token_forge)
    {
        return Resolution::Ready {
            forge: token_forge,
            means: Means::Token,
        };
    }
    if let Some(forge) = known {
        return Resolution::NotConnected { forge };
    }
    if probes.is_github_enterprise(host) {
        return Resolution::NotConnected {
            forge: Forge::GitHub,
        };
    }
    Resolution::UnknownForge
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Asks the real system: each CLI's own sign-in state, and the host's
/// unauthenticated GitHub Enterprise metadata endpoint.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProbes;

impl Probes for SystemProbes {
    fn cli_signed_in(&self, forge: Forge, host: &str) -> bool {
        matches!(
            run(
                CliProgram::for_forge(forge),
                &["auth", "status", "--hostname", host],
                None,
                PROBE_TIMEOUT
            ),
            Ok(CliOutput { code: Some(0), .. })
        )
    }

    /// GitHub Enterprise Server answers `GET /api/v3/meta` without
    /// authentication and names its `installed_version`; github.com's meta
    /// does not, and a GitLab answers 404.
    fn is_github_enterprise(&self, host: &str) -> bool {
        let url = format!("{}/api/v3/meta", web_base(host));
        let Ok(response) = http_agent(PROBE_TIMEOUT).get(&url).call() else {
            return false;
        };
        if response.status().as_u16() != 200 {
            return false;
        }
        let Ok(body) = response.into_body().read_to_vec() else {
            return false;
        };
        serde_json::from_slice::<serde_json::Value>(&body)
            .is_ok_and(|meta| meta.get("installed_version").is_some())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Answers from lists, and records every question in order.
    #[derive(Default)]
    struct Fake {
        gh: Vec<&'static str>,
        glab: Vec<&'static str>,
        enterprise: Vec<&'static str>,
        asked: RefCell<Vec<String>>,
    }

    impl Probes for Fake {
        fn cli_signed_in(&self, forge: Forge, host: &str) -> bool {
            self.asked
                .borrow_mut()
                .push(format!("{forge:?} cli {host}"));
            match forge {
                Forge::GitHub => self.gh.contains(&host),
                Forge::GitLab => self.glab.contains(&host),
            }
        }

        fn is_github_enterprise(&self, host: &str) -> bool {
            self.asked.borrow_mut().push(format!("meta {host}"));
            self.enterprise.contains(&host)
        }
    }

    impl Fake {
        fn asked(&self) -> Vec<String> {
            self.asked.borrow().clone()
        }
    }

    fn setting(host: &str, forge: Forge, means: Option<Means>) -> HostSetting {
        HostSetting {
            host: host.to_string(),
            forge,
            means,
        }
    }

    #[test]
    fn a_setting_with_a_means_is_obeyed_without_asking_the_system() {
        let fake = Fake {
            glab: vec!["git.corp"],
            ..Fake::default()
        };
        let chosen = setting("git.corp", Forge::GitHub, Some(Means::Cli));
        assert_eq!(
            resolve("git.corp", Some(&chosen), None, &fake),
            Resolution::Ready {
                forge: Forge::GitHub,
                means: Means::Cli
            }
        );
        assert!(fake.asked().is_empty(), "asked {:?}", fake.asked());
    }

    #[test]
    fn a_token_setting_without_a_token_is_not_connected_rather_than_cli() {
        let fake = Fake {
            gh: vec!["git.corp"],
            ..Fake::default()
        };
        let chosen = setting("git.corp", Forge::GitHub, Some(Means::Token));
        assert_eq!(
            resolve("git.corp", Some(&chosen), None, &fake),
            Resolution::NotConnected {
                forge: Forge::GitHub
            }
        );
    }

    #[test]
    fn github_dot_com_never_asks_glab() {
        let fake = Fake {
            glab: vec!["github.com"],
            ..Fake::default()
        };
        assert_eq!(
            resolve("github.com", None, None, &fake),
            Resolution::NotConnected {
                forge: Forge::GitHub
            }
        );
        assert_eq!(fake.asked(), vec!["GitHub cli github.com".to_string()]);
    }

    #[test]
    fn a_signed_in_glab_names_an_unknown_host_without_the_meta_probe() {
        let fake = Fake {
            glab: vec!["git.corp"],
            ..Fake::default()
        };
        assert_eq!(
            resolve("git.corp", None, None, &fake),
            Resolution::Ready {
                forge: Forge::GitLab,
                means: Means::Cli
            }
        );
        assert!(
            !fake
                .asked()
                .iter()
                .any(|question| question.starts_with("meta"))
        );
    }

    #[test]
    fn a_signed_in_cli_is_preferred_to_a_stored_token() {
        let fake = Fake {
            gh: vec!["github.com"],
            ..Fake::default()
        };
        assert_eq!(
            resolve("github.com", None, Some(Forge::GitHub), &fake),
            Resolution::Ready {
                forge: Forge::GitHub,
                means: Means::Cli
            }
        );
    }

    #[test]
    fn a_stored_token_names_an_unknown_host_without_the_meta_probe() {
        let fake = Fake::default();
        assert_eq!(
            resolve("git.corp", None, Some(Forge::GitLab), &fake),
            Resolution::Ready {
                forge: Forge::GitLab,
                means: Means::Token
            }
        );
        assert!(
            !fake
                .asked()
                .iter()
                .any(|question| question.starts_with("meta"))
        );
    }

    #[test]
    fn a_token_saved_for_the_other_forge_is_ignored() {
        let fake = Fake::default();
        let chosen = setting("git.corp", Forge::GitLab, None);
        assert_eq!(
            resolve("git.corp", Some(&chosen), Some(Forge::GitHub), &fake),
            Resolution::NotConnected {
                forge: Forge::GitLab
            }
        );
    }

    #[test]
    fn a_known_host_with_nothing_is_not_connected_and_never_probed() {
        let fake = Fake::default();
        assert_eq!(
            resolve("gitlab.com", None, None, &fake),
            Resolution::NotConnected {
                forge: Forge::GitLab
            }
        );
        assert!(
            !fake
                .asked()
                .iter()
                .any(|question| question.starts_with("meta"))
        );
    }

    #[test]
    fn an_unknown_host_answering_meta_is_github_enterprise() {
        let fake = Fake {
            enterprise: vec!["ghe.corp"],
            ..Fake::default()
        };
        assert_eq!(
            resolve("ghe.corp", None, None, &fake),
            Resolution::NotConnected {
                forge: Forge::GitHub
            }
        );
    }

    #[test]
    fn an_unknown_host_with_nothing_is_unknown() {
        assert_eq!(
            resolve("git.corp", None, None, &Fake::default()),
            Resolution::UnknownForge
        );
    }

    #[test]
    fn a_forge_only_setting_detects_the_means_for_that_forge_alone() {
        let fake = Fake {
            gh: vec!["git.corp"],
            glab: vec!["git.corp"],
            ..Fake::default()
        };
        let chosen = setting("git.corp", Forge::GitLab, None);
        assert_eq!(
            resolve("git.corp", Some(&chosen), None, &fake),
            Resolution::Ready {
                forge: Forge::GitLab,
                means: Means::Cli
            }
        );
        assert_eq!(fake.asked(), vec!["GitLab cli git.corp".to_string()]);
    }
}

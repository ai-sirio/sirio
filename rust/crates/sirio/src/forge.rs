//! The host's side of the change-request seam
//! (`sirio_ui::forge_source::ChangeRequestSource`): which forge a worktree's
//! remote is on, how Sirio signs in there, and the one `ForgeClient` per
//! project the UI reads through. What the user decided — a host's forge,
//! its means — is stored under the settings key `forge.hosts`; a token lives
//! in the credential store under `forge:<host>`, never anywhere else.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sirio_forge::{
    ChangeRef, CliProgram, CliTransport, Forge, ForgeClient, ForgeError, ForgeTarget, HostSetting,
    Means, Resolution, SystemProbes, TokenTransport, Transport, parse_remote_url, resolve,
};
use sirio_ui::forge_source::{ChangeRequestSource, Connection, HostRow, ReadyConnection};
use sirio_usage::CredentialStore;

use crate::session::SessionStore;

pub(crate) fn source_owner(
    forge: Forge,
    listed: &ForgeTarget,
    origin: Option<&ForgeTarget>,
) -> Option<String> {
    let origin = origin?;
    if origin.host != listed.host || origin.project == listed.project {
        return None;
    }
    match forge {
        Forge::GitHub => origin.project.split('/').next().map(str::to_string),
        Forge::GitLab => Some(origin.project.clone()),
    }
}

/// A token as the credential store keeps it: with the forge it was saved
/// for, so resolution can name an unknown host without probing it.
#[derive(Serialize, Deserialize)]
struct StoredToken {
    forge: Forge,
    token: String,
}

fn token_key(host: &str) -> String {
    format!("forge:{host}")
}

/// One client per host, project and means, so a client's cached viewer and
/// GitLab baseline flag survive refreshes and worktree switches.
type ClientKey = (String, String, bool);

fn client_key(target: &ForgeTarget, means: Means) -> ClientKey {
    (
        target.host.clone(),
        target.project.clone(),
        means == Means::Cli,
    )
}

pub(crate) struct ForgeHub {
    settings: SessionStore,
    credentials: Option<CredentialStore>,
    resolutions: Mutex<HashMap<String, Resolution>>,
    clients: Mutex<HashMap<ClientKey, Arc<ForgeClient>>>,
}

impl ForgeHub {
    pub(crate) fn new(settings: SessionStore, credentials: Option<CredentialStore>) -> Self {
        Self {
            settings,
            credentials,
            resolutions: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn host_settings(&self) -> Vec<HostSetting> {
        serde_json::from_str(&self.settings.load_settings().forge_hosts).unwrap_or_default()
    }

    /// Load, change, save: `forge.hosts` is owned here, not by the Settings
    /// snapshot (see `app_settings_for_settings_save`).
    fn update_host_settings(&self, change: impl FnOnce(&mut Vec<HostSetting>)) {
        let mut settings = self.settings.load_settings();
        let mut hosts: Vec<HostSetting> =
            serde_json::from_str(&settings.forge_hosts).unwrap_or_default();
        change(&mut hosts);
        settings.forge_hosts = serde_json::to_string(&hosts).unwrap_or_else(|_| "[]".to_string());
        self.settings.save_settings(&settings);
    }

    fn stored_token(&self, host: &str) -> Option<StoredToken> {
        let raw = self.credentials.as_ref()?.get(&token_key(host))?;
        serde_json::from_str(&raw).ok()
    }

    fn resolution(&self, host: &str) -> Resolution {
        if let Some(found) = self.resolutions.lock().expect("resolutions lock").get(host) {
            return *found;
        }
        let setting = self
            .host_settings()
            .into_iter()
            .find(|setting| setting.host == host);
        let stored = self.stored_token(host).map(|stored| stored.forge);
        let resolution = resolve(host, setting.as_ref(), stored, &SystemProbes);
        self.resolutions
            .lock()
            .expect("resolutions lock")
            .insert(host.to_string(), resolution);
        resolution
    }

    fn client(
        &self,
        forge: Forge,
        means: Means,
        target: ForgeTarget,
    ) -> Result<Arc<ForgeClient>, Connection> {
        let key = client_key(&target, means);
        if let Some(client) = self.clients.lock().expect("clients lock").get(&key) {
            return Ok(client.clone());
        }
        let transport: Box<dyn Transport> = match means {
            Means::Cli => Box::new(CliTransport::new(
                CliProgram::for_forge(forge),
                &target.host,
            )),
            Means::Token => match self.stored_token(&target.host) {
                Some(stored) => Box::new(TokenTransport::new(forge, &target.host, stored.token)),
                None => {
                    return Err(Connection::NotConnected {
                        forge,
                        host: target.host,
                    });
                }
            },
        };
        let client = Arc::new(ForgeClient::new(forge, target, transport));
        self.clients
            .lock()
            .expect("clients lock")
            .insert(key, client.clone());
        Ok(client)
    }

    /// Drop what was resolved and built for `host`; the next call asks again.
    fn drop_host(&self, host: &str) {
        self.resolutions
            .lock()
            .expect("resolutions lock")
            .remove(host);
        self.clients
            .lock()
            .expect("clients lock")
            .retain(|(client_host, _, _), _| client_host != host);
    }
}

impl ChangeRequestSource for ForgeHub {
    fn connect(&self, worktree: &Path) -> Connection {
        let origin = sirio_git::remote_url(worktree, "origin");
        let listed = sirio_git::remote_url(worktree, "upstream").or_else(|| origin.clone());
        let Some(target) = listed.as_deref().and_then(parse_remote_url) else {
            return Connection::NoForgeRemote;
        };
        match self.resolution(&target.host) {
            Resolution::UnknownForge => Connection::UnknownForge { host: target.host },
            Resolution::NotConnected { forge } => Connection::NotConnected {
                forge,
                host: target.host,
            },
            Resolution::Ready { forge, means } => {
                let origin = origin.as_deref().and_then(parse_remote_url);
                let source_owner = source_owner(forge, &target, origin.as_ref());
                match self.client(forge, means, target) {
                    Ok(client) => Connection::Ready(ReadyConnection {
                        client,
                        means,
                        branch: sirio_project::current_branch(worktree).ok().flatten(),
                        source_owner,
                    }),
                    Err(connection) => connection,
                }
            }
        }
    }

    fn client_for(&self, reference: &ChangeRef) -> Result<Arc<ForgeClient>, Connection> {
        match self.resolution(&reference.host) {
            Resolution::Ready { forge, means } if forge == reference.forge => self.client(
                forge,
                means,
                ForgeTarget {
                    host: reference.host.clone(),
                    project: reference.project.clone(),
                },
            ),
            Resolution::UnknownForge => Err(Connection::UnknownForge {
                host: reference.host.clone(),
            }),
            _ => Err(Connection::NotConnected {
                forge: reference.forge,
                host: reference.host.clone(),
            }),
        }
    }

    fn forget(&self, host: &str) {
        self.drop_host(host);
    }

    fn set_forge(&self, host: &str, forge: Forge) {
        self.update_host_settings(|hosts| {
            match hosts.iter_mut().find(|setting| setting.host == host) {
                Some(existing) if existing.forge != forge => {
                    existing.forge = forge;
                    existing.means = None;
                }
                Some(_) => {}
                None => hosts.push(HostSetting {
                    host: host.to_string(),
                    forge,
                    means: None,
                }),
            }
        });
        self.drop_host(host);
    }

    fn set_means(&self, host: &str, forge: Forge, means: Option<Means>) {
        self.update_host_settings(|hosts| {
            match hosts.iter_mut().find(|setting| setting.host == host) {
                Some(existing) => {
                    existing.forge = forge;
                    existing.means = means;
                }
                None => hosts.push(HostSetting {
                    host: host.to_string(),
                    forge,
                    means,
                }),
            }
        });
        self.drop_host(host);
    }

    fn save_token(&self, host: &str, forge: Forge, token: &str) -> Result<String, ForgeError> {
        let token = token.trim();
        let Some(store) = &self.credentials else {
            return Err(ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: "there is nowhere to keep a token: no HOME or XDG_DATA_HOME".to_string(),
            });
        };
        let probe = ForgeClient::new(
            forge,
            ForgeTarget {
                host: host.to_string(),
                project: String::new(),
            },
            Box::new(TokenTransport::new(forge, host, token.to_string())),
        );
        let account = probe.viewer()?;
        let value = serde_json::to_string(&StoredToken {
            forge,
            token: token.to_string(),
        })
        .expect("a stored token serialises");
        store
            .set(&token_key(host), &value)
            .map_err(|error| ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: error.to_string(),
            })?;
        self.drop_host(host);
        Ok(account)
    }

    fn delete_token(&self, host: &str) {
        if let Some(store) = &self.credentials {
            let _ = store.delete(&token_key(host));
        }
        self.drop_host(host);
    }

    fn hosts(&self) -> Vec<HostRow> {
        let settings = self.host_settings();
        let mut names: Vec<String> = settings
            .iter()
            .map(|setting| setting.host.clone())
            .collect();
        for host in self.resolutions.lock().expect("resolutions lock").keys() {
            if !names.contains(host) {
                names.push(host.clone());
            }
        }
        names.sort();
        names
            .into_iter()
            .map(|host| {
                let setting = settings.iter().find(|setting| setting.host == host);
                let (forge, means) = match self.resolution(&host) {
                    Resolution::Ready { forge, means } => (Some(forge), Some(means)),
                    Resolution::NotConnected { forge } => (Some(forge), None),
                    Resolution::UnknownForge => (setting.map(|setting| setting.forge), None),
                };
                let account = match (forge, means) {
                    (Some(forge), Some(means)) => self
                        .client(
                            forge,
                            means,
                            ForgeTarget {
                                host: host.clone(),
                                project: String::new(),
                            },
                        )
                        .ok()
                        .and_then(|client| client.viewer().ok()),
                    _ => None,
                };
                HostRow {
                    has_token: self.stored_token(&host).is_some(),
                    configured: setting.is_some(),
                    pinned_means: setting.and_then(|setting| setting.means),
                    host,
                    forge,
                    means,
                    account,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(host: &str, project: &str) -> ForgeTarget {
        ForgeTarget {
            host: host.to_string(),
            project: project.to_string(),
        }
    }

    #[test]
    fn a_fork_on_github_is_named_by_its_owner() {
        let listed = target("github.com", "acme/widgets");
        let origin = target("github.com", "me/widgets");
        assert_eq!(
            source_owner(Forge::GitHub, &listed, Some(&origin)).as_deref(),
            Some("me")
        );
    }

    #[test]
    fn a_fork_on_gitlab_is_named_by_its_full_path() {
        let listed = target("gitlab.example.com", "team/sub/app");
        let origin = target("gitlab.example.com", "me/forks/app");
        assert_eq!(
            source_owner(Forge::GitLab, &listed, Some(&origin)).as_deref(),
            Some("me/forks/app")
        );
    }

    #[test]
    fn listing_origin_itself_needs_no_owner() {
        let listed = target("github.com", "acme/widgets");
        assert_eq!(
            source_owner(Forge::GitHub, &listed, Some(&listed.clone())),
            None
        );
        assert_eq!(source_owner(Forge::GitHub, &listed, None), None);
    }

    #[test]
    fn an_origin_on_another_host_says_nothing_about_this_forge() {
        let listed = target("github.com", "acme/widgets");
        let origin = target("gitlab.com", "me/widgets");
        assert_eq!(source_owner(Forge::GitHub, &listed, Some(&origin)), None);
    }
}

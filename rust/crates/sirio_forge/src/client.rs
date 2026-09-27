//! The one type the app holds: a forge, a project on it, and a transport.

use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use crate::error::ForgeError;
use crate::model::{ChangeRef, Forge};
use crate::target::ForgeTarget;
use crate::transport::Transport;
use crate::{github, gitlab};

/// A connection to one project on one forge. `Send + Sync`: the app shares
/// one per project across background tasks. Every method blocks.
pub struct ForgeClient {
    pub(crate) forge: Forge,
    pub(crate) host: String,
    pub(crate) project: String,
    pub(crate) transport: Box<dyn Transport>,
    viewer: OnceLock<String>,
    /// GitLab only: set once the server rejected a newer field, so every
    /// later query goes straight to its baseline variant (spec §6.3).
    pub(crate) baseline: AtomicBool,
}

impl ForgeClient {
    pub fn new(forge: Forge, target: ForgeTarget, transport: Box<dyn Transport>) -> Self {
        Self {
            forge,
            host: target.host,
            project: target.project,
            transport,
            viewer: OnceLock::new(),
            baseline: AtomicBool::new(false),
        }
    }

    pub fn forge(&self) -> Forge {
        self.forge
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// The identity of change request `number` on this project.
    pub fn reference(&self, number: u64) -> ChangeRef {
        ChangeRef {
            forge: self.forge,
            host: self.host.clone(),
            project: self.project.clone(),
            number,
        }
    }

    /// The signed-in account's login — read once, then kept.
    pub fn viewer(&self) -> Result<String, ForgeError> {
        if let Some(login) = self.viewer.get() {
            return Ok(login.clone());
        }
        let login = match self.forge {
            Forge::GitHub => github::viewer(self)?,
            Forge::GitLab => gitlab::viewer(self)?,
        };
        Ok(self.viewer.get_or_init(|| login).clone())
    }
}

impl std::fmt::Debug for ForgeClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForgeClient")
            .field("forge", &self.forge)
            .field("host", &self.host)
            .field("project", &self.project)
            .finish_non_exhaustive()
    }
}

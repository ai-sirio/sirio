//! The seam between the UI and the host for change requests (spec §4).
//!
//! The UI asks "what is this worktree connected to?" and gets back a client
//! it can read with. How the host found the forge, where a token lives and
//! how the means was chosen stay the host's (`sirio::forge::ForgeHub`). A
//! GPUI global, set once by the host, because the right panel is rebuilt on
//! every worktree switch and a detail tab outlives any one panel.

use std::path::Path;
use std::sync::Arc;

use gpui::{App, Global};
use sirio_forge::{ChangeRef, Forge, ForgeClient, ForgeError, Means};

/// What a worktree's remote is connected to.
#[derive(Clone)]
pub enum Connection {
    /// No `upstream` or `origin` remote on a forge.
    NoForgeRemote,
    /// A remote on `host` whose forge nothing identifies: the user must say.
    UnknownForge {
        host: String,
    },
    /// The forge is known, but there is no signed-in CLI and no token.
    NotConnected {
        forge: Forge,
        host: String,
    },
    Ready(ReadyConnection),
}

#[derive(Clone)]
pub struct ReadyConnection {
    pub client: Arc<ForgeClient>,
    pub means: Means,
    /// The worktree's branch; `None` on a detached HEAD, which has no card.
    pub branch: Option<String>,
    /// For `ForgeClient::for_branch`: set only when the list reads another
    /// project (`upstream`) than the user's `origin` fork (spec §6.5).
    pub source_owner: Option<String>,
}

/// One host as Settings → Git hosting shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostRow {
    pub host: String,
    /// `None`: nothing identified it and the user has not said.
    pub forge: Option<Forge>,
    /// The means in use; `None` when not connected.
    pub means: Option<Means>,
    /// The means the user pinned in Settings; `None` means "detect".
    pub pinned_means: Option<Means>,
    pub account: Option<String>,
    pub has_token: bool,
    /// The user configured this host (it has a `forge.hosts` entry).
    pub configured: bool,
}

/// What the UI may ask the host. Every method may block — running `gh` or
/// `glab`, probing a host, reading the credential store — so callers run
/// them on the background executor.
pub trait ChangeRequestSource: Send + Sync {
    fn connect(&self, worktree: &Path) -> Connection;
    /// A client for a change request a tab was opened or restored with.
    fn client_for(&self, reference: &ChangeRef) -> Result<Arc<ForgeClient>, Connection>;
    /// Forget what was resolved for `host`, so the next call asks again.
    fn forget(&self, host: &str);
    /// The user's answer to "which forge is this host?". Persisted.
    fn set_forge(&self, host: &str, forge: Forge);
    /// Pin a means for `host`, or `None` to detect it again. Persisted.
    fn set_means(&self, host: &str, forge: Forge, means: Option<Means>);
    /// Verifies `token` against the forge, then stores it; returns the
    /// account it signs in as. A token the forge rejects is never stored.
    fn save_token(&self, host: &str, forge: Forge, token: &str) -> Result<String, ForgeError>;
    fn delete_token(&self, host: &str);
    /// Every host seen this session or configured.
    fn hosts(&self) -> Vec<HostRow>;
}

struct SourceGlobal(Arc<dyn ChangeRequestSource>);

impl Global for SourceGlobal {}

pub fn source(cx: &App) -> Option<Arc<dyn ChangeRequestSource>> {
    cx.try_global::<SourceGlobal>()
        .map(|global| global.0.clone())
}

pub fn set_source(source: Arc<dyn ChangeRequestSource>, cx: &mut App) {
    cx.set_global(SourceGlobal(source));
}

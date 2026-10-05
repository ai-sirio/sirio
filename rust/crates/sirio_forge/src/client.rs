//! The one type the app holds: a forge, a project on it, and a transport.

use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use crate::action::{Action, ActionOutcome};
use crate::error::ForgeError;
use crate::model::{
    Candidate, ChangeHeader, ChangePage, ChangeRef, ChangeState, ChangeSummary, Check, CheckJob, Log, CommitSummary,
    FileChange, Forge, ListQuery, Listing, PageCursor, ReviewThread,
};
use crate::scopes::TokenScopes;
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

/// A detail list stops after this many requests and says it was cut.
pub(crate) const MAX_PAGES: usize = 10;

impl ForgeClient {
    /// One page of the list for `query`; pass the previous page's `next` to
    /// continue it.
    pub fn list(
        &self,
        query: &ListQuery,
        cursor: Option<&PageCursor>,
    ) -> Result<ChangePage, ForgeError> {
        match self.forge {
            Forge::GitHub => github::list(self, query, cursor),
            Forge::GitLab => gitlab::list(self, query, cursor),
        }
    }

    /// Open change requests waiting on the signed-in user's review.
    pub fn to_review_count(&self) -> Result<u32, ForgeError> {
        match self.forge {
            Forge::GitHub => github::to_review_count(self),
            Forge::GitLab => gitlab::to_review_count(self),
        }
    }

    /// The change request whose source is `branch` (spec §6.5).
    pub fn for_branch(
        &self,
        branch: &str,
        source_owner: Option<&str>,
    ) -> Result<Option<ChangeSummary>, ForgeError> {
        match self.forge {
            Forge::GitHub => github::for_branch(self, branch, source_owner),
            Forge::GitLab => gitlab::for_branch(self, branch, source_owner),
        }
    }

    pub fn header(&self, number: u64) -> Result<ChangeHeader, ForgeError> {
        match self.forge {
            Forge::GitHub => github::header(self, number),
            Forge::GitLab => gitlab::header(self, number),
        }
    }

    pub fn commits(&self, number: u64) -> Result<Listing<CommitSummary>, ForgeError> {
        match self.forge {
            Forge::GitHub => github::commits(self, number),
            Forge::GitLab => gitlab::commits(self, number),
        }
    }

    pub fn checks(&self, number: u64) -> Result<Listing<Check>, ForgeError> {
        match self.forge {
            Forge::GitHub => github::checks(self, number),
            Forge::GitLab => gitlab::checks(self, number),
        }
    }

    /// The tail of a CI job's log (spec §7.4). Two requests: the job's
    /// status, then its log.
    pub fn job_log(&self, job: &CheckJob) -> Result<Log, ForgeError> {
        let _perf = sirio_perf::span("forge.job_log", 0);
        match self.forge {
            Forge::GitHub => github::job_log(self, job),
            Forge::GitLab => gitlab::job_log(self, job),
        }
    }

    pub fn files(&self, number: u64) -> Result<Listing<FileChange>, ForgeError> {
        match self.forge {
            Forge::GitHub => github::files(self, number),
            Forge::GitLab => gitlab::files(self, number),
        }
    }

    /// Every review thread of the change request, paged (spec §4). Read when
    /// *Files* opens and on a manual refresh, never polled.
    pub fn review_threads(&self, number: u64) -> Result<Listing<ReviewThread>, ForgeError> {
        let _perf = sirio_perf::span("forge.review_threads", 0);
        match self.forge {
            Forge::GitHub => github::review_threads(self, number),
            Forge::GitLab => gitlab::review_threads(self, number),
        }
    }

    /// The one door for a write (spec §5). Reads the change request's
    /// permissions afresh, refuses what the forge would refuse, sends the
    /// rest — and never retries it: a write that may have been sent is not
    /// sent again by anything but the user.
    pub fn act(&self, number: u64, action: &Action) -> Result<ActionOutcome, ForgeError> {
        let _perf = sirio_perf::span("forge.act", 0);
        match self.forge {
            Forge::GitHub => github::act(self, number, action),
            Forge::GitLab => gitlab::act(self, number, action),
        }
    }

    /// People who may be asked to review change request `number`, found by
    /// the forge from `text` (not a local filter: an organisation's first
    /// page may not hold the one sought). Ids are what the reviewer
    /// mutation names.
    pub fn reviewer_candidates(&self, number: u64, text: &str) -> Result<Vec<Candidate>, ForgeError> {
        let _perf = sirio_perf::span("forge.reviewer_candidates", 0);
        match self.forge {
            Forge::GitHub => github::reviewer_candidates(self, number, text),
            Forge::GitLab => gitlab::reviewer_candidates(self, text),
        }
    }

    /// The project's labels matching `text`, with the ids the label
    /// mutation names.
    pub fn label_candidates(&self, text: &str) -> Result<Vec<Candidate>, ForgeError> {
        let _perf = sirio_perf::span("forge.label_candidates", 0);
        match self.forge {
            Forge::GitHub => github::label_candidates(self, text),
            Forge::GitLab => gitlab::label_candidates(self, text),
        }
    }

    /// The scopes the signed-in token reports for itself, for Settings. `None`
    /// where the forge does not say — a fine-grained GitHub token sends no
    /// `X-OAuth-Scopes`, and a GitLab OAuth token has no personal access token
    /// record — and on any failure: a question about a token's scopes never
    /// becomes an error of its own.
    pub fn token_scopes(&self) -> Option<TokenScopes> {
        match self.forge {
            Forge::GitHub => github::token_scopes(self),
            Forge::GitLab => gitlab::token_scopes(self),
        }
    }

    /// The forge's own page for opening a change request from `branch`,
    /// prefilled. A browser link: it never goes through the test endpoint.
    pub fn creation_url(&self, branch: &str) -> String {
        match self.forge {
            Forge::GitHub => format!(
                "https://{}/{}/compare/{}?expand=1",
                self.host,
                self.project,
                percent_encode(branch, true)
            ),
            Forge::GitLab => format!(
                "https://{}/{}/-/merge_requests/new?merge_request%5Bsource_branch%5D={}",
                self.host,
                self.project,
                percent_encode(branch, false)
            ),
        }
    }
}

/// One list page from one or more connections: a change request both
/// authored and assigned appears once, newest first; `next` is `None` once
/// every connection is exhausted. Across a *Load more* a union is only
/// roughly ordered — each page is sorted, and the caller drops a number it
/// already holds.
pub(crate) fn page(mut items: Vec<ChangeSummary>, slots: Vec<Option<String>>) -> ChangePage {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.reference.number));
    items.sort_by_key(|left| std::cmp::Reverse(left.updated_at));
    let next = slots
        .iter()
        .any(Option::is_some)
        .then_some(PageCursor { slots });
    ChangePage { items, next }
}

/// The worktree's own change request (spec §6.5). With an owner, only one
/// from that owner's project counts — a deleted fork (`None`) never does.
/// Among the rest, the first open one, else the most recent (the forge
/// answers newest first).
pub(crate) fn pick_for_branch(
    candidates: Vec<ChangeSummary>,
    source_owner: Option<&str>,
) -> Option<ChangeSummary> {
    let mut candidates: Vec<ChangeSummary> = candidates
        .into_iter()
        .filter(|candidate| match source_owner {
            None => true,
            Some(owner) => candidate
                .source_owner
                .as_deref()
                .is_some_and(|found| found.eq_ignore_ascii_case(owner)),
        })
        .collect();
    let open = candidates
        .iter()
        .position(|candidate| matches!(candidate.state, ChangeState::Open | ChangeState::Draft));
    match open {
        Some(index) => Some(candidates.swap_remove(index)),
        None => candidates.into_iter().next(),
    }
}

/// Pages a detail list: at most [`MAX_PAGES`] requests, then `truncated`.
pub(crate) fn paged<T>(
    mut fetch: impl FnMut(Option<&str>) -> Result<(Vec<T>, Option<String>), ForgeError>,
) -> Result<Listing<T>, ForgeError> {
    let mut items = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let (page, next) = fetch(after.as_deref())?;
        items.extend(page);
        match next {
            Some(cursor) => after = Some(cursor),
            None => {
                return Ok(Listing {
                    items,
                    truncated: false,
                });
            }
        }
    }
    Ok(Listing {
        items,
        truncated: true,
    })
}

/// Percent-encodes everything but RFC 3986's unreserved characters, and
/// `/` when `keep_slash`.
pub(crate) fn percent_encode(text: &str, keep_slash: bool) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (keep_slash && byte == b'/');
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

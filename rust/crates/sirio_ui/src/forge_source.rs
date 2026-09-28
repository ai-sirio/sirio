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
    /// The branch `worktree` has checked out now; `None` on a detached HEAD.
    /// `connect` reads it once, so a panel that stays open asks again.
    fn current_branch(&self, worktree: &Path) -> Option<String>;
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

/// A forge in memory for the UI's tests: a real `ForgeClient` over a
/// transport that answers each GraphQL operation with what the test set.
#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use sirio_forge::{
        ApiResponse, ChangeRef, Forge, ForgeClient, ForgeError, ForgeTarget, Means, Transport,
    };

    use super::{ChangeRequestSource, Connection, HostRow, ReadyConnection};

    /// Never blocks: GPUI's test dispatcher polls background work on the
    /// test's own thread, so a transport that waited for the test to release
    /// it would deadlock. Races are staged by queueing loads before pumping.
    #[derive(Default)]
    pub(crate) struct CannedForge {
        answers: Mutex<HashMap<String, (u16, Vec<(String, String)>, String)>>,
        seen: Mutex<Vec<String>>,
    }

    impl CannedForge {
        pub(crate) fn answer(&self, operation: &str, body: String) {
            self.answers
                .lock()
                .unwrap()
                .insert(operation.to_string(), (200, Vec::new(), body));
        }

        pub(crate) fn fail(&self, operation: &str, status: u16) {
            self.answers.lock().unwrap().insert(
                operation.to_string(),
                (status, Vec::new(), r#"{"message":"failed"}"#.to_string()),
            );
        }

        pub(crate) fn rate_limited(&self, operation: &str, reset_at: i64) {
            self.answers.lock().unwrap().insert(
                operation.to_string(),
                (
                    429,
                    vec![("x-ratelimit-reset".to_string(), reset_at.to_string())],
                    r#"{"message":"rate limited"}"#.to_string(),
                ),
            );
        }

        pub(crate) fn count(&self, operation: &str) -> usize {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|seen| *seen == operation)
                .count()
        }
    }

    struct Shared(Arc<CannedForge>);

    impl Transport for Shared {
        fn post_graphql(&self, body: &[u8]) -> Result<ApiResponse, ForgeError> {
            let request: Value = serde_json::from_slice(body).expect("a JSON request");
            let operation = request["operationName"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            self.0.seen.lock().unwrap().push(operation.clone());
            let (status, headers, body) = self
                .0
                .answers
                .lock()
                .unwrap()
                .get(&operation)
                .cloned()
                .unwrap_or((404, Vec::new(), "{}".to_string()));
            Ok(ApiResponse {
                status,
                headers,
                body: body.into_bytes(),
            })
        }
    }

    pub(crate) fn github_client(forge: Arc<CannedForge>) -> Arc<ForgeClient> {
        Arc::new(ForgeClient::new(
            Forge::GitHub,
            ForgeTarget {
                host: "ghe.test".to_string(),
                project: "acme/widgets".to_string(),
            },
            Box::new(Shared(forge)),
        ))
    }

    pub(crate) fn reference(number: u64) -> ChangeRef {
        ChangeRef {
            forge: Forge::GitHub,
            host: "ghe.test".to_string(),
            project: "acme/widgets".to_string(),
            number,
        }
    }

    pub(crate) fn viewer() -> String {
        json!({"data": {"viewer": {"login": "me"}}}).to_string()
    }

    /// One pull request in the shape `summary.graphql` asks for.
    pub(crate) fn summary(number: u64, title: &str) -> Value {
        json!({
            "number": number, "title": title,
            "url": format!("https://ghe.test/acme/widgets/pull/{number}"),
            "isDraft": false, "state": "OPEN", "updatedAt": "2026-09-27T10:00:00Z",
            "author": {"login": "alice"}, "headRefName": format!("feat/{number}"), "baseRefName": "main",
            "headRepositoryOwner": {"login": "acme"}, "comments": {"totalCount": 2},
            "reviewDecision": null, "approvals": {"totalCount": 0}, "reviewRequests": {"nodes": []},
            "commits": {"nodes": []}
        })
    }

    pub(crate) fn list(nodes: Vec<Value>, next: Option<&str>) -> String {
        json!({"data": {"repository": {"pullRequests": {
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}, "nodes": nodes
        }}}})
        .to_string()
    }

    pub(crate) fn search(nodes: Vec<Value>) -> String {
        json!({"data": {"search": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes}}})
            .to_string()
    }

    pub(crate) fn count(count: u32) -> String {
        json!({"data": {"search": {"issueCount": count}}}).to_string()
    }

    pub(crate) fn branch(nodes: Vec<Value>) -> String {
        json!({"data": {"repository": {"pullRequests": {"nodes": nodes}}}}).to_string()
    }

    pub(crate) fn header(number: u64, title: &str, body: &str) -> String {
        let mut node = summary(number, title);
        let extra = json!({
            "body": body, "additions": 10, "deletions": 2, "changedFiles": 1,
            "allCommits": {"totalCount": 1},
            "timelineItems": {"pageInfo": {"hasPreviousPage": false, "startCursor": null}, "nodes": [
                {"__typename": "IssueComment", "author": {"login": "bob"}, "body": "Looks good.", "createdAt": "2026-09-27T09:00:00Z"}
            ]}
        });
        for (key, value) in extra.as_object().expect("an object") {
            node[key] = value.clone();
        }
        json!({"data": {"repository": {"pullRequest": node}}}).to_string()
    }

    pub(crate) fn commits() -> String {
        json!({"data": {"repository": {"pullRequest": {"commits": {
            "totalCount": 1, "pageInfo": {"hasNextPage": false, "endCursor": null},
            "nodes": [{"commit": {"oid": "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "abbreviatedOid": "1111111",
                "messageHeadline": "Fix it", "committedDate": "2026-09-27T09:00:00Z",
                "url": "https://ghe.test/acme/widgets/commit/1111111", "author": {"name": "Alice", "user": {"login": "alice"}}}}]
        }}}}})
        .to_string()
    }

    /// A source whose every answer the test decides, recording what the UI
    /// asked of it.
    pub(crate) struct FakeSource {
        connection: Mutex<Connection>,
        pub(crate) forgotten: Mutex<Vec<String>>,
        pub(crate) forges: Mutex<Vec<(String, Forge)>>,
        pub(crate) tokens: Mutex<Vec<(String, Forge, String)>>,
        pub(crate) token_answer: Mutex<Result<String, ForgeError>>,
        pub(crate) connects: Mutex<u32>,
        /// What `git` says the worktree has checked out right now.
        head: Mutex<Option<String>>,
    }

    impl FakeSource {
        pub(crate) fn with(connection: Connection) -> Arc<Self> {
            let head = match &connection {
                Connection::Ready(ready) => ready.branch.clone(),
                _ => None,
            };
            Arc::new(Self {
                head: Mutex::new(head),
                connection: Mutex::new(connection),
                forgotten: Mutex::new(Vec::new()),
                forges: Mutex::new(Vec::new()),
                tokens: Mutex::new(Vec::new()),
                token_answer: Mutex::new(Ok("me".to_string())),
                connects: Mutex::new(0),
            })
        }

        pub(crate) fn ready(client: Arc<ForgeClient>, branch: Option<&str>) -> Arc<Self> {
            Self::with(Connection::Ready(ReadyConnection {
                client,
                means: Means::Token,
                branch: branch.map(str::to_string),
                source_owner: None,
            }))
        }

        pub(crate) fn set(&self, connection: Connection) {
            *self.connection.lock().unwrap() = connection;
        }

        /// Checks another branch out, as a `git switch` would.
        pub(crate) fn switch_to(&self, branch: &str) {
            *self.head.lock().unwrap() = Some(branch.to_string());
        }
    }

    impl ChangeRequestSource for FakeSource {
        fn connect(&self, _worktree: &Path) -> Connection {
            *self.connects.lock().unwrap() += 1;
            self.connection.lock().unwrap().clone()
        }

        fn current_branch(&self, _worktree: &Path) -> Option<String> {
            self.head.lock().unwrap().clone()
        }

        fn client_for(&self, _reference: &ChangeRef) -> Result<Arc<ForgeClient>, Connection> {
            match self.connection.lock().unwrap().clone() {
                Connection::Ready(ready) => Ok(ready.client),
                other => Err(other),
            }
        }

        fn forget(&self, host: &str) {
            self.forgotten.lock().unwrap().push(host.to_string());
        }

        fn set_forge(&self, host: &str, forge: Forge) {
            self.forges.lock().unwrap().push((host.to_string(), forge));
        }

        fn set_means(&self, _host: &str, _forge: Forge, _means: Option<Means>) {}

        fn save_token(&self, host: &str, forge: Forge, token: &str) -> Result<String, ForgeError> {
            self.tokens
                .lock()
                .unwrap()
                .push((host.to_string(), forge, token.to_string()));
            self.token_answer.lock().unwrap().clone()
        }

        fn delete_token(&self, _host: &str) {}

        fn hosts(&self) -> Vec<HostRow> {
            Vec::new()
        }
    }
}

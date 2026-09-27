//! GitLab's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/gitlab/` was validated against gitlab.com on
//! 2026-09-27 (`curl https://gitlab.com/api/graphql`, project
//! gitlab-org/cli). Change a field only after running the changed query
//! there again.

use serde_json::json;

use crate::client::ForgeClient;
use crate::error::ForgeError;
use crate::graphql::{execute, no_unknown_field, opt_str};
use crate::model::{
    ChangeHeader, ChangePage, ChangeSummary, Check, CommitSummary, FileChange, ListQuery, Listing,
    PageCursor,
};

const CURRENT_USER: &str = include_str!("queries/gitlab/current_user.graphql");

/// GitLab answers an anonymous request with `currentUser: null`.
pub(crate) fn viewer(client: &ForgeClient) -> Result<String, ForgeError> {
    let data = execute(client, "CurrentUser", CURRENT_USER, json!({})).map_err(no_unknown_field)?;
    opt_str(&data, "/currentUser/username")
        .map(str::to_string)
        .ok_or_else(|| ForgeError::NotAuthenticated {
            host: client.host.clone(),
        })
}

fn not_yet(client: &ForgeClient) -> ForgeError {
    ForgeError::UnexpectedResponse {
        host: client.host.clone(),
        detail: "GitLab reads arrive in Task 7".to_string(),
    }
}

pub(crate) fn list(
    client: &ForgeClient,
    _: &ListQuery,
    _: Option<&PageCursor>,
) -> Result<ChangePage, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn to_review_count(client: &ForgeClient) -> Result<u32, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn for_branch(
    client: &ForgeClient,
    _: &str,
    _: Option<&str>,
) -> Result<Option<ChangeSummary>, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn header(client: &ForgeClient, _: u64) -> Result<ChangeHeader, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn commits(client: &ForgeClient, _: u64) -> Result<Listing<CommitSummary>, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn checks(client: &ForgeClient, _: u64) -> Result<Listing<Check>, ForgeError> {
    Err(not_yet(client))
}
pub(crate) fn files(client: &ForgeClient, _: u64) -> Result<Listing<FileChange>, ForgeError> {
    Err(not_yet(client))
}

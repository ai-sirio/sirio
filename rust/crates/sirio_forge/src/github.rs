//! GitHub's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/github/` was validated against github.com on
//! 2026-09-27 (`gh api graphql`, repository ai-sirio/sirio). Change a field
//! only after running the changed query there again.

use serde_json::{Value, json};

use crate::client::ForgeClient;
use crate::error::ForgeError;
use crate::graphql::{execute, no_unknown_field, opt_str};

const VIEWER: &str = include_str!("queries/github/viewer.graphql");

/// A GitHub query. GitHub has no baseline to fall back to, so a field the
/// server does not know is an answer Sirio cannot read.
fn run(
    client: &ForgeClient,
    operation: &str,
    text: &str,
    variables: Value,
) -> Result<Value, ForgeError> {
    execute(client, operation, text, variables).map_err(no_unknown_field)
}

pub(crate) fn viewer(client: &ForgeClient) -> Result<String, ForgeError> {
    let data = run(client, "Viewer", VIEWER, json!({}))?;
    opt_str(&data, "/viewer/login")
        .map(str::to_string)
        .ok_or_else(|| ForgeError::NotAuthenticated {
            host: client.host.clone(),
        })
}

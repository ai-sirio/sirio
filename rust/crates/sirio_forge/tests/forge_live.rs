//! Live conformance: the queries Sirio sends, against the real forges, so
//! the day one stops matching the live schema is noticed here rather than
//! by a user. The fixtures of `Scripts/Tests/test-forge-e2e.sh` freeze
//! today's shape; only a real call sees tomorrow's.
//!
//! It SKIPs without credentials, in the style of
//! `sirio_agents/tests/acp_conformance.rs`, so an unverified claim never
//! turns into a false green; and it is on the release gate's skip list
//! beside those tests (`rust/.config/nextest.toml`): a release must not
//! depend on the network.
//!
//! GitHub: `gh` signed in to github.com, against ai-sirio/sirio (public).
//! GitLab: `glab` signed in to gitlab.com, or `SIRIO_FORGE_LIVE_GITLAB_TOKEN`
//! set to a `read_api` token, against gitlab-org/cli (public).

use sirio_forge::{
    ChangeState, CliProgram, CliTransport, Filter, Forge, ForgeClient, ForgeTarget, ListQuery,
    TokenTransport, Transport,
};

fn cli_signed_in(program: &str, host: &str) -> bool {
    std::process::Command::new(program)
        .args(["auth", "status", "--hostname", host])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn github_answers_every_query_sirio_sends() {
    if !cli_signed_in("gh", "github.com") {
        eprintln!("SKIP: gh is not signed in to github.com");
        return;
    }
    let client = ForgeClient::new(
        Forge::GitHub,
        ForgeTarget {
            host: "github.com".to_string(),
            project: "ai-sirio/sirio".to_string(),
        },
        Box::new(CliTransport::new(CliProgram::Gh, "github.com")),
    );
    exercise(&client);
}

fn gitlab_transport() -> Option<Box<dyn Transport>> {
    if cli_signed_in("glab", "gitlab.com") {
        Some(Box::new(CliTransport::new(CliProgram::Glab, "gitlab.com")))
    } else if let Ok(token) = std::env::var("SIRIO_FORGE_LIVE_GITLAB_TOKEN") {
        Some(Box::new(TokenTransport::new(Forge::GitLab, "gitlab.com", token)))
    } else {
        eprintln!(
            "SKIP: glab is not signed in to gitlab.com and SIRIO_FORGE_LIVE_GITLAB_TOKEN is unset"
        );
        None
    }
}

#[test]
fn gitlab_answers_every_query_sirio_sends() {
    let Some(transport) = gitlab_transport() else {
        return;
    };
    let client = ForgeClient::new(
        Forge::GitLab,
        ForgeTarget {
            host: "gitlab.com".to_string(),
            project: "gitlab-org/cli".to_string(),
        },
        transport,
    );
    exercise(&client);
}

/// Every read, against a real project with history: each list answers, and
/// the most recently merged change request answers every detail query.
fn exercise(client: &ForgeClient) {
    client.viewer().expect("viewer");
    for filter in [
        Filter::Mine,
        Filter::ToReview,
        Filter::AllOpen,
        Filter::ClosedAndMerged,
    ] {
        client
            .list(
                &ListQuery {
                    filter,
                    search: None,
                },
                None,
            )
            .unwrap_or_else(|error| panic!("{filter:?}: {error}"));
    }
    client
        .list(
            &ListQuery {
                filter: Filter::AllOpen,
                search: Some("fix".to_string()),
            },
            None,
        )
        .expect("search");
    client.to_review_count().expect("count");
    let closed = client
        .list(
            &ListQuery {
                filter: Filter::ClosedAndMerged,
                search: None,
            },
            None,
        )
        .expect("closed and merged");
    let merged = closed
        .items
        .iter()
        .find(|item| item.state == ChangeState::Merged)
        .expect("the project has a merged change request");
    client
        .for_branch(&merged.source_branch, None)
        .expect("for_branch");
    let header = client.header(merged.reference.number).expect("header");
    assert_eq!(header.summary.reference, merged.reference);
    assert!(!header.summary.title.is_empty());
    client.commits(merged.reference.number).expect("commits");
    client.checks(merged.reference.number).expect("checks");
    client.files(merged.reference.number).expect("files");
}

/// Sends every document `act` uses exactly as it is, with variables that
/// change nothing (`sirio_forge::live_probes`), and requires the forge to
/// accept the *document*. A forge answers a mutation on an id it does not hold
/// with a runtime error — GitHub's `NOT_FOUND`, GitLab's "resource not
/// available" — which carries no `extensions`; an unknown field, a misspelt
/// input, a bad enum value or a missing argument is a validation error, and
/// carries them. That is the day a mutation stops matching the live schema.
fn assert_documents_are_valid(transport: &dyn Transport, forge: Forge) {
    for probe in sirio_forge::live_probes(forge) {
        let body = serde_json::json!({
            "operationName": probe.operation,
            "query": probe.document,
            "variables": probe.variables,
        });
        let response = transport
            .post_graphql(&serde_json::to_vec(&body).expect("a JSON value serialises"))
            .unwrap_or_else(|error| panic!("{}: {error}", probe.operation));
        let answer: serde_json::Value = serde_json::from_slice(&response.body)
            .unwrap_or_else(|error| panic!("{}: not JSON: {error}", probe.operation));
        let refused: Vec<&serde_json::Value> = answer["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|error| error.get("extensions").is_some() && error.get("type").is_none())
            .collect();
        assert!(
            refused.is_empty(),
            "{}: the live schema refuses this document: {refused:?}",
            probe.operation
        );
    }
}

#[test]
fn github_accepts_every_document_sirio_writes_with() {
    if !cli_signed_in("gh", "github.com") {
        eprintln!("SKIP: gh is not signed in to github.com");
        return;
    }
    assert_documents_are_valid(&CliTransport::new(CliProgram::Gh, "github.com"), Forge::GitHub);
}

#[test]
fn gitlab_accepts_every_document_sirio_writes_with() {
    let Some(transport) = gitlab_transport() else {
        return;
    };
    assert_documents_are_valid(transport.as_ref(), Forge::GitLab);
}

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

#[test]
fn gitlab_answers_every_query_sirio_sends() {
    let transport: Box<dyn Transport> = if cli_signed_in("glab", "gitlab.com") {
        Box::new(CliTransport::new(CliProgram::Glab, "gitlab.com"))
    } else if let Ok(token) = std::env::var("SIRIO_FORGE_LIVE_GITLAB_TOKEN") {
        Box::new(TokenTransport::new(Forge::GitLab, "gitlab.com", token))
    } else {
        eprintln!(
            "SKIP: glab is not signed in to gitlab.com and SIRIO_FORGE_LIVE_GITLAB_TOKEN is unset"
        );
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

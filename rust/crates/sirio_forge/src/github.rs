//! GitHub's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/github/` was validated against github.com on
//! 2026-09-27 (`gh api graphql`, repository ai-sirio/sirio); the header
//! query's `baseRefOid`/`headRefOid` fields were added and run there again
//! on 2026-09-28 (pull request #578). Change a field only after running the
//! changed query there again.

use serde_json::{Value, json};

use crate::client::{ForgeClient, page, paged, pick_for_branch};
use crate::error::ForgeError;
use crate::graphql::{
    array_at, bool_at, execute, has_previous_page, next_cursor, no_unknown_field, opt_str, opt_u32,
    revisions, str_at, time_at, u32_at,
};
use crate::mapping;
use crate::model::{
    ChangeHeader, ChangePage, ChangeSummary, Check, CiState, CommitSummary, EventKind, FileChange,
    Filter, LineComment, ListQuery, Listing, PageCursor, ReviewOutcome, Reviewer, TimelineItem,
};

macro_rules! with_summary {
    ($file:literal) => {
        concat!(
            include_str!($file),
            "\n",
            include_str!("queries/github/summary.graphql")
        )
    };
}

const VIEWER: &str = include_str!("queries/github/viewer.graphql");
const LIST: &str = with_summary!("queries/github/list.graphql");
const SEARCH: &str = with_summary!("queries/github/search.graphql");
const MINE: &str = with_summary!("queries/github/mine.graphql");
const COUNT: &str = include_str!("queries/github/count.graphql");
const BRANCH: &str = with_summary!("queries/github/branch.graphql");
const HEADER: &str = with_summary!("queries/github/header.graphql");
const COMMITS: &str = include_str!("queries/github/commits.graphql");
const CHECKS: &str = include_str!("queries/github/checks.graphql");
const FILES: &str = include_str!("queries/github/files.graphql");

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

/// GitHub has no subgroups: a project is `owner/name`.
fn owner_and_name(client: &ForgeClient) -> Result<(&str, &str), ForgeError> {
    match client.project.split_once('/') {
        Some((owner, name)) if !name.contains('/') => Ok((owner, name)),
        _ => Err(ForgeError::NotFound {
            host: client.host.clone(),
        }),
    }
}

/// A deleted account's content has `author: null`; GitHub shows it as
/// "ghost".
fn login_or_ghost(node: &Value, pointer: &str) -> String {
    opt_str(node, pointer).unwrap_or("ghost").to_string()
}

fn state_counts<'a>(rollup: &'a Value, pointer: &str) -> Vec<mapping::StateCount<'a>> {
    array_at(rollup, pointer)
        .into_iter()
        .filter_map(|pair| {
            Some(mapping::StateCount {
                state: pair.get("state")?.as_str()?,
                count: u32::try_from(pair.get("count")?.as_u64()?).ok()?,
            })
        })
        .collect()
}

/// A list row, or `None` for a node without a number or title — a search
/// hit that is not a pull request answers `{}` (spec §10: skip, don't fail).
fn summary(client: &ForgeClient, node: &Value, viewer: &str) -> Option<ChangeSummary> {
    let number = node.get("number")?.as_u64()?;
    let title = node.get("title")?.as_str()?.to_string();
    let rollup = array_at(node, "/commits/nodes")
        .last()
        .and_then(|commit| commit.pointer("/commit/statusCheckRollup"))
        .filter(|rollup| !rollup.is_null());
    let ci = match rollup {
        None => CiState::NoChecks,
        Some(rollup) => mapping::github_ci(
            opt_str(rollup, "/state"),
            &state_counts(rollup, "/contexts/checkRunCountsByState"),
            &state_counts(rollup, "/contexts/statusContextCountsByState"),
        ),
    };
    let review_requested_from_me =
        array_at(node, "/reviewRequests/nodes")
            .into_iter()
            .any(|request| {
                opt_str(request, "/requestedReviewer/login")
                    .is_some_and(|login| login.eq_ignore_ascii_case(viewer))
            });
    Some(ChangeSummary {
        reference: client.reference(number),
        title,
        author: login_or_ghost(node, "/author/login"),
        state: mapping::github_change_state(&str_at(node, "/state"), bool_at(node, "/isDraft")),
        ci,
        review: mapping::github_review(
            opt_str(node, "/reviewDecision"),
            u32_at(node, "/approvals/totalCount"),
        ),
        review_requested_from_me,
        comments: u32_at(node, "/comments/totalCount"),
        source_branch: str_at(node, "/headRefName"),
        target_branch: str_at(node, "/baseRefName"),
        source_owner: opt_str(node, "/headRepositoryOwner/login").map(str::to_string),
        updated_at: time_at(node, "/updatedAt"),
        web_url: str_at(node, "/url"),
    })
}

pub(crate) fn list(
    client: &ForgeClient,
    query: &ListQuery,
    cursor: Option<&PageCursor>,
) -> Result<ChangePage, ForgeError> {
    let viewer = client.viewer()?;
    let text = query.search_text();
    match (query.filter, text) {
        (Filter::Mine, _) => mine(client, text, cursor, &viewer),
        (Filter::AllOpen, None) => repository_list(client, &["OPEN"], cursor, &viewer),
        (Filter::ClosedAndMerged, None) => {
            repository_list(client, &["MERGED", "CLOSED"], cursor, &viewer)
        }
        (filter, _) => {
            let q = search_query(&client.project, filter, text);
            let data = run(
                client,
                "ChangeRequestSearch",
                SEARCH,
                json!({ "q": q, "after": cursor.and_then(|cursor| cursor.slot(0)) }),
            )?;
            Ok(single_page(client, &data, "/search", &viewer))
        }
    }
}

fn search_query(project: &str, filter: Filter, text: Option<&str>) -> String {
    let scope = match filter {
        Filter::ClosedAndMerged => "is:pr is:closed",
        Filter::ToReview => "is:pr is:open review-requested:@me",
        Filter::Mine | Filter::AllOpen => "is:pr is:open",
    };
    match text {
        Some(text) => format!("repo:{project} {scope} {text} sort:updated-desc"),
        None => format!("repo:{project} {scope} sort:updated-desc"),
    }
}

fn repository_list(
    client: &ForgeClient,
    states: &[&str],
    cursor: Option<&PageCursor>,
    viewer: &str,
) -> Result<ChangePage, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "ChangeRequestList",
        LIST,
        json!({
            "owner": owner,
            "name": name,
            "states": states,
            "after": cursor.and_then(|cursor| cursor.slot(0)),
        }),
    )?;
    Ok(single_page(
        client,
        &data,
        "/repository/pullRequests",
        viewer,
    ))
}

fn single_page(client: &ForgeClient, data: &Value, connection: &str, viewer: &str) -> ChangePage {
    let items = array_at(data, &format!("{connection}/nodes"))
        .into_iter()
        .filter_map(|node| summary(client, node, viewer))
        .collect();
    page(items, vec![next_cursor(data, connection)])
}

/// *Mine*: authored ∪ assigned, one request with two aliased searches. On a
/// later page an exhausted side is left out with `@include(if: false)`.
fn mine(
    client: &ForgeClient,
    text: Option<&str>,
    cursor: Option<&PageCursor>,
    viewer: &str,
) -> Result<ChangePage, ForgeError> {
    let (with_authored, after_authored, with_assigned, after_assigned) = match cursor {
        None => (true, None, true, None),
        Some(cursor) => (
            cursor.slot(0).is_some(),
            cursor.slot(0),
            cursor.slot(1).is_some(),
            cursor.slot(1),
        ),
    };
    let project = &client.project;
    let text = text.map(|text| format!(" {text}")).unwrap_or_default();
    let data = run(
        client,
        "ChangeRequestMine",
        MINE,
        json!({
            "authored": format!("repo:{project} is:pr is:open author:@me{text} sort:updated-desc"),
            "assigned": format!("repo:{project} is:pr is:open assignee:@me{text} sort:updated-desc"),
            "afterAuthored": after_authored,
            "afterAssigned": after_assigned,
            "withAuthored": with_authored,
            "withAssigned": with_assigned,
        }),
    )?;
    let mut items = Vec::new();
    let mut slots = Vec::new();
    for alias in ["/authored", "/assigned"] {
        items.extend(
            array_at(&data, &format!("{alias}/nodes"))
                .into_iter()
                .filter_map(|node| summary(client, node, viewer)),
        );
        slots.push(next_cursor(&data, alias));
    }
    Ok(page(items, slots))
}

pub(crate) fn to_review_count(client: &ForgeClient) -> Result<u32, ForgeError> {
    let q = format!("repo:{} is:pr is:open review-requested:@me", client.project);
    let data = run(client, "ChangeRequestCount", COUNT, json!({ "q": q }))?;
    Ok(u32_at(&data, "/search/issueCount"))
}

pub(crate) fn for_branch(
    client: &ForgeClient,
    branch: &str,
    source_owner: Option<&str>,
) -> Result<Option<ChangeSummary>, ForgeError> {
    let viewer = client.viewer()?;
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "ChangeRequestForBranch",
        BRANCH,
        json!({ "owner": owner, "name": name, "branch": branch }),
    )?;
    let candidates = array_at(&data, "/repository/pullRequests/nodes")
        .into_iter()
        .filter_map(|node| summary(client, node, &viewer))
        .collect();
    Ok(pick_for_branch(candidates, source_owner))
}

pub(crate) fn header(client: &ForgeClient, number: u64) -> Result<ChangeHeader, ForgeError> {
    let viewer = client.viewer()?;
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "ChangeRequestHeader",
        HEADER,
        json!({ "owner": owner, "name": name, "number": number }),
    )?;
    let node = data
        .pointer("/repository/pullRequest")
        .filter(|node| !node.is_null())
        .ok_or_else(|| ForgeError::NotFound {
            host: client.host.clone(),
        })?;
    let summary = summary(client, node, &viewer).ok_or_else(|| ForgeError::UnexpectedResponse {
        host: client.host.clone(),
        detail: "a pull request without a number or title".to_string(),
    })?;
    let timeline = timeline(&array_at(node, "/timelineItems/nodes"));
    let reviewers = reviewers(&timeline, &array_at(node, "/reviewRequests/nodes"));
    Ok(ChangeHeader {
        summary,
        body: str_at(node, "/body"),
        reviewers,
        additions: opt_u32(node, "/additions"),
        deletions: opt_u32(node, "/deletions"),
        changed_files: opt_u32(node, "/changedFiles"),
        commit_count: opt_u32(node, "/allCommits/totalCount"),
        timeline_truncated: has_previous_page(node, "/timelineItems"),
        timeline,
        revisions: revisions(opt_str(node, "/baseRefOid"), opt_str(node, "/headRefOid"), None),
    })
}

fn event(node: &Value, kind: EventKind) -> TimelineItem {
    TimelineItem::Event {
        actor: opt_str(node, "/actor/login").map(str::to_string),
        kind,
        at: time_at(node, "/createdAt"),
    }
}

/// Consecutive commits fold into one "N commits" event; a pending review
/// (the viewer's own draft) is left out; a type Sirio does not know keeps
/// its name.
fn timeline(nodes: &[&Value]) -> Vec<TimelineItem> {
    let mut items: Vec<TimelineItem> = Vec::new();
    for node in nodes {
        match opt_str(node, "/__typename").unwrap_or("") {
            "PullRequestCommit" => {
                let when = time_at(node, "/commit/committedDate");
                if let Some(TimelineItem::Event {
                    kind: EventKind::CommitsPushed { count },
                    at,
                    ..
                }) = items.last_mut()
                {
                    *count += 1;
                    *at = when.or(*at);
                    continue;
                }
                items.push(TimelineItem::Event {
                    actor: None,
                    kind: EventKind::CommitsPushed { count: 1 },
                    at: when,
                });
            }
            "IssueComment" => items.push(TimelineItem::Comment {
                author: login_or_ghost(node, "/author/login"),
                body: str_at(node, "/body"),
                at: time_at(node, "/createdAt"),
            }),
            "PullRequestReview" => {
                let Some(outcome) = mapping::github_review_outcome(&str_at(node, "/state")) else {
                    continue;
                };
                let author = login_or_ghost(node, "/author/login");
                let when = time_at(node, "/submittedAt");
                let line_comments = array_at(node, "/comments/nodes")
                    .into_iter()
                    .map(|comment| LineComment {
                        author: author.clone(),
                        path: str_at(comment, "/path"),
                        line: opt_u32(comment, "/line")
                            .or_else(|| opt_u32(comment, "/originalLine")),
                        body: str_at(comment, "/body"),
                        at: when,
                    })
                    .collect();
                items.push(TimelineItem::Review {
                    author,
                    outcome,
                    body: str_at(node, "/body"),
                    at: when,
                    line_comments,
                });
            }
            "ReviewRequestedEvent" => {
                let reviewer = opt_str(node, "/requestedReviewer/login")
                    .or_else(|| opt_str(node, "/requestedReviewer/slug"))
                    .unwrap_or_default()
                    .to_string();
                items.push(event(node, EventKind::ReviewRequested { reviewer }));
            }
            "MergedEvent" => items.push(event(node, EventKind::Merged)),
            "ClosedEvent" => items.push(event(node, EventKind::Closed)),
            "ReopenedEvent" => items.push(event(node, EventKind::Reopened)),
            "ReadyForReviewEvent" => items.push(event(node, EventKind::ReadyForReview)),
            "ConvertToDraftEvent" => items.push(event(node, EventKind::ConvertedToDraft)),
            "" => {}
            other => items.push(event(node, EventKind::Other(other.to_string()))),
        }
    }
    items
}

/// Each reviewer's latest word — a later "commented" does not erase an
/// approval or a request for changes — then everyone whose review is
/// pending, which a re-request makes true again.
fn reviewers(timeline: &[TimelineItem], requests: &[&Value]) -> Vec<Reviewer> {
    let mut reviewers: Vec<Reviewer> = Vec::new();
    for item in timeline {
        let TimelineItem::Review {
            author, outcome, ..
        } = item
        else {
            continue;
        };
        match reviewers
            .iter_mut()
            .find(|reviewer| reviewer.login == *author)
        {
            Some(existing) => {
                let decided = matches!(
                    existing.outcome,
                    ReviewOutcome::Approved | ReviewOutcome::ChangesRequested
                );
                if !(decided && *outcome == ReviewOutcome::Commented) {
                    existing.outcome = *outcome;
                }
            }
            None => reviewers.push(Reviewer {
                login: author.clone(),
                outcome: *outcome,
            }),
        }
    }
    for request in requests {
        let Some(login) = opt_str(request, "/requestedReviewer/login")
            .or_else(|| opt_str(request, "/requestedReviewer/slug"))
        else {
            continue;
        };
        match reviewers
            .iter_mut()
            .find(|reviewer| reviewer.login == login)
        {
            Some(existing) => existing.outcome = ReviewOutcome::Requested,
            None => reviewers.push(Reviewer {
                login: login.to_string(),
                outcome: ReviewOutcome::Requested,
            }),
        }
    }
    reviewers
}

pub(crate) fn commits(
    client: &ForgeClient,
    number: u64,
) -> Result<Listing<CommitSummary>, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    paged(|after| {
        let data = run(
            client,
            "ChangeRequestCommits",
            COMMITS,
            json!({ "owner": owner, "name": name, "number": number, "after": after }),
        )?;
        let connection = "/repository/pullRequest/commits";
        let items = array_at(&data, &format!("{connection}/nodes"))
            .into_iter()
            .filter_map(|node| {
                let commit = node.get("commit")?;
                Some(CommitSummary {
                    sha: opt_str(commit, "/oid")?.to_string(),
                    short_sha: str_at(commit, "/abbreviatedOid"),
                    title: str_at(commit, "/messageHeadline"),
                    author: opt_str(commit, "/author/user/login")
                        .or_else(|| opt_str(commit, "/author/name"))
                        .unwrap_or("ghost")
                        .to_string(),
                    at: time_at(commit, "/committedDate"),
                    web_url: str_at(commit, "/url"),
                })
            })
            .collect();
        Ok((items, next_cursor(&data, connection)))
    })
}

pub(crate) fn checks(client: &ForgeClient, number: u64) -> Result<Listing<Check>, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    paged(|after| {
        let data = run(
            client,
            "ChangeRequestChecks",
            CHECKS,
            json!({ "owner": owner, "name": name, "number": number, "after": after }),
        )?;
        let connection =
            "/repository/pullRequest/commits/nodes/0/commit/statusCheckRollup/contexts";
        let items = array_at(&data, &format!("{connection}/nodes"))
            .into_iter()
            .filter_map(check)
            .collect();
        Ok((items, next_cursor(&data, connection)))
    })
}

fn check(node: &Value) -> Option<Check> {
    match opt_str(node, "/__typename")? {
        "CheckRun" => Some(Check {
            name: opt_str(node, "/name")?.to_string(),
            status: mapping::github_check_run(
                &str_at(node, "/status"),
                opt_str(node, "/conclusion"),
            ),
            group: opt_str(node, "/checkSuite/workflowRun/workflow/name").map(str::to_string),
            duration_secs: duration(time_at(node, "/startedAt"), time_at(node, "/completedAt")),
            url: opt_str(node, "/detailsUrl").map(str::to_string),
        }),
        "StatusContext" => Some(Check {
            name: opt_str(node, "/context")?.to_string(),
            status: mapping::github_status_context(&str_at(node, "/state")),
            group: None,
            duration_secs: None,
            url: opt_str(node, "/targetUrl").map(str::to_string),
        }),
        _ => None,
    }
}

fn duration(started: Option<i64>, finished: Option<i64>) -> Option<u64> {
    u64::try_from(finished? - started?).ok()
}

pub(crate) fn files(client: &ForgeClient, number: u64) -> Result<Listing<FileChange>, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    paged(|after| {
        let data = run(
            client,
            "ChangeRequestFiles",
            FILES,
            json!({ "owner": owner, "name": name, "number": number, "after": after }),
        )?;
        let connection = "/repository/pullRequest/files";
        let items = array_at(&data, &format!("{connection}/nodes"))
            .into_iter()
            .filter_map(|node| {
                Some(FileChange {
                    path: opt_str(node, "/path")?.to_string(),
                    kind: mapping::file_change_kind(&str_at(node, "/changeType")),
                    additions: u32_at(node, "/additions"),
                    deletions: u32_at(node, "/deletions"),
                })
            })
            .collect();
        Ok((items, next_cursor(&data, connection)))
    })
}

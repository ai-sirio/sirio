//! GitLab's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/gitlab/` was validated against gitlab.com on
//! 2026-09-27 (`curl https://gitlab.com/api/graphql`, project
//! gitlab-org/cli). Change a field only after running the changed query
//! there again.
//!
//! GraphQL rejects a whole query that names a field the server lacks, and
//! self-managed installations run old versions. So the queries that carry
//! a merge request summary have a baseline variant without the newest
//! fields: the first `Field '…' doesn't exist` switches the client to the
//! baseline for its lifetime (spec §6.3).

use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::client::{ForgeClient, page, paged, pick_for_branch};
use crate::error::ForgeError;
use crate::graphql::{
    array_at, bool_at, execute, has_previous_page, next_cursor, no_unknown_field, opt_str, opt_u32,
    str_at, time_at, u32_at,
};
use crate::mapping::{self, SystemNote};
use crate::model::{
    ChangeHeader, ChangePage, ChangeSummary, Check, CommitSummary, FileChange, Filter, LineComment,
    ListQuery, Listing, PageCursor, ReviewOutcome, Reviewer, TimelineItem,
};

macro_rules! full {
    ($file:literal) => {
        concat!(
            include_str!($file),
            "\n",
            include_str!("queries/gitlab/summary.graphql")
        )
    };
}

macro_rules! baseline {
    ($file:literal) => {
        concat!(
            include_str!($file),
            "\n",
            include_str!("queries/gitlab/summary_baseline.graphql")
        )
    };
}

const CURRENT_USER: &str = include_str!("queries/gitlab/current_user.graphql");
const LIST: (&str, &str) = (
    full!("queries/gitlab/list.graphql"),
    baseline!("queries/gitlab/list.graphql"),
);
const UNION: (&str, &str) = (
    full!("queries/gitlab/union.graphql"),
    baseline!("queries/gitlab/union.graphql"),
);
const BRANCH: (&str, &str) = (
    full!("queries/gitlab/branch.graphql"),
    baseline!("queries/gitlab/branch.graphql"),
);
const HEADER: (&str, &str) = (
    full!("queries/gitlab/header.graphql"),
    baseline!("queries/gitlab/header_baseline.graphql"),
);
const COUNT: &str = include_str!("queries/gitlab/count.graphql");
const COMMITS: &str = include_str!("queries/gitlab/commits.graphql");
const CHECKS: &str = include_str!("queries/gitlab/checks.graphql");
const FILES: &str = include_str!("queries/gitlab/files.graphql");

/// The full query, or its baseline once this server has rejected a newer
/// field; after the first rejection every later call goes straight to the
/// baseline.
fn run(
    client: &ForgeClient,
    operation: &str,
    (full, baseline): (&str, &str),
    variables: Value,
) -> Result<Value, ForgeError> {
    if !client.baseline.load(Ordering::Relaxed) {
        match execute(client, operation, full, variables.clone()) {
            Err(ForgeError::UnknownField { .. }) => client.baseline.store(true, Ordering::Relaxed),
            answer => return answer,
        }
    }
    execute(client, operation, baseline, variables).map_err(no_unknown_field)
}

/// A query with no baseline.
fn plain(
    client: &ForgeClient,
    operation: &str,
    text: &str,
    variables: Value,
) -> Result<Value, ForgeError> {
    execute(client, operation, text, variables).map_err(no_unknown_field)
}

fn not_found(client: &ForgeClient) -> ForgeError {
    ForgeError::NotFound {
        host: client.host.clone(),
    }
}

/// `project` is `null` when the project does not exist or is not visible.
fn project<'a>(client: &ForgeClient, data: &'a Value) -> Result<&'a Value, ForgeError> {
    data.get("project")
        .filter(|project| !project.is_null())
        .ok_or_else(|| not_found(client))
}

fn merge_request<'a>(client: &ForgeClient, data: &'a Value) -> Result<&'a Value, ForgeError> {
    project(client, data)?
        .get("mergeRequest")
        .filter(|merge_request| !merge_request.is_null())
        .ok_or_else(|| not_found(client))
}

/// GitLab answers an anonymous request with `currentUser: null`.
pub(crate) fn viewer(client: &ForgeClient) -> Result<String, ForgeError> {
    let data = plain(client, "CurrentUser", CURRENT_USER, json!({}))?;
    opt_str(&data, "/currentUser/username")
        .map(str::to_string)
        .ok_or_else(|| ForgeError::NotAuthenticated {
            host: client.host.clone(),
        })
}

/// A list row, or `None` for a node without an iid or title (spec §10).
fn summary(client: &ForgeClient, node: &Value, me: &str) -> Option<ChangeSummary> {
    let number: u64 = node.get("iid")?.as_str()?.parse().ok()?;
    let title = node.get("title")?.as_str()?.to_string();
    let reviewers: Vec<(&str, Option<&str>)> = array_at(node, "/reviewers/nodes")
        .into_iter()
        .filter_map(|reviewer| {
            Some((
                opt_str(reviewer, "/username")?,
                opt_str(reviewer, "/mergeRequestInteraction/reviewState"),
            ))
        })
        .collect();
    let reviewers: Vec<mapping::GitLabReviewer<'_>> = reviewers
        .into_iter()
        .map(|(username, review_state)| mapping::GitLabReviewer {
            username,
            review_state,
        })
        .collect();
    let approvals = u32::try_from(array_at(node, "/approvedBy/nodes").len()).unwrap_or(u32::MAX);
    Some(ChangeSummary {
        reference: client.reference(number),
        title,
        author: opt_str(node, "/author/username")
            .unwrap_or("ghost")
            .to_string(),
        state: mapping::gitlab_change_state(&str_at(node, "/state"), bool_at(node, "/draft")),
        ci: mapping::gitlab_ci(
            opt_str(node, "/headPipeline/status"),
            opt_u32(node, "/headPipeline/jobs/count"),
            opt_u32(node, "/headPipeline/finished/count"),
        ),
        review: mapping::gitlab_review(&reviewers, approvals),
        review_requested_from_me: mapping::gitlab_review_requested_from(&reviewers, me),
        comments: u32_at(node, "/userNotesCount"),
        source_branch: str_at(node, "/sourceBranch"),
        target_branch: str_at(node, "/targetBranch"),
        source_owner: opt_str(node, "/sourceProject/fullPath").map(str::to_string),
        updated_at: time_at(node, "/updatedAt"),
        web_url: str_at(node, "/webUrl"),
    })
}

/// One side of a union: which merge requests it asks for.
struct Side<'a> {
    state: &'static str,
    author: Option<&'a str>,
    assignee: Option<&'a str>,
}

pub(crate) fn list(
    client: &ForgeClient,
    query: &ListQuery,
    cursor: Option<&PageCursor>,
) -> Result<ChangePage, ForgeError> {
    let me = client.viewer()?;
    let search = query.search_text();
    match query.filter {
        Filter::AllOpen => single(client, None, search, cursor, &me),
        Filter::ToReview => single(client, Some(&me), search, cursor, &me),
        Filter::Mine => union(
            client,
            search,
            [
                Side {
                    state: "opened",
                    author: Some(&me),
                    assignee: None,
                },
                Side {
                    state: "opened",
                    author: None,
                    assignee: Some(&me),
                },
            ],
            cursor,
            &me,
        ),
        Filter::ClosedAndMerged => union(
            client,
            search,
            [
                Side {
                    state: "merged",
                    author: None,
                    assignee: None,
                },
                Side {
                    state: "closed",
                    author: None,
                    assignee: None,
                },
            ],
            cursor,
            &me,
        ),
    }
}

fn single(
    client: &ForgeClient,
    reviewer: Option<&str>,
    search: Option<&str>,
    cursor: Option<&PageCursor>,
    me: &str,
) -> Result<ChangePage, ForgeError> {
    let data = run(
        client,
        "MergeRequestList",
        LIST,
        json!({
            "fullPath": client.project,
            "state": "opened",
            "reviewer": reviewer,
            "search": search,
            "after": cursor.and_then(|cursor| cursor.slot(0)),
        }),
    )?;
    let project = project(client, &data)?;
    let items = array_at(project, "/mergeRequests/nodes")
        .into_iter()
        .filter_map(|node| summary(client, node, me))
        .collect();
    Ok(page(items, vec![next_cursor(project, "/mergeRequests")]))
}

fn union(
    client: &ForgeClient,
    search: Option<&str>,
    [a, b]: [Side<'_>; 2],
    cursor: Option<&PageCursor>,
    me: &str,
) -> Result<ChangePage, ForgeError> {
    let (with_a, after_a, with_b, after_b) = match cursor {
        None => (true, None, true, None),
        Some(cursor) => (
            cursor.slot(0).is_some(),
            cursor.slot(0),
            cursor.slot(1).is_some(),
            cursor.slot(1),
        ),
    };
    let data = run(
        client,
        "MergeRequestUnion",
        UNION,
        json!({
            "fullPath": client.project,
            "search": search,
            "aState": a.state, "aAuthor": a.author, "aAssignee": a.assignee, "aAfter": after_a, "withA": with_a,
            "bState": b.state, "bAuthor": b.author, "bAssignee": b.assignee, "bAfter": after_b, "withB": with_b,
        }),
    )?;
    let project = project(client, &data)?;
    let mut items = Vec::new();
    let mut slots = Vec::new();
    for alias in ["/a", "/b"] {
        items.extend(
            array_at(project, &format!("{alias}/nodes"))
                .into_iter()
                .filter_map(|node| summary(client, node, me)),
        );
        slots.push(next_cursor(project, alias));
    }
    Ok(page(items, slots))
}

pub(crate) fn to_review_count(client: &ForgeClient) -> Result<u32, ForgeError> {
    let me = client.viewer()?;
    let data = plain(
        client,
        "MergeRequestCount",
        COUNT,
        json!({ "fullPath": client.project, "reviewer": me }),
    )?;
    Ok(u32_at(project(client, &data)?, "/mergeRequests/count"))
}

pub(crate) fn for_branch(
    client: &ForgeClient,
    branch: &str,
    source_owner: Option<&str>,
) -> Result<Option<ChangeSummary>, ForgeError> {
    let me = client.viewer()?;
    let data = run(
        client,
        "MergeRequestForBranch",
        BRANCH,
        json!({ "fullPath": client.project, "branch": branch }),
    )?;
    let candidates = array_at(project(client, &data)?, "/mergeRequests/nodes")
        .into_iter()
        .filter_map(|node| summary(client, node, &me))
        .collect();
    Ok(pick_for_branch(candidates, source_owner))
}

pub(crate) fn header(client: &ForgeClient, number: u64) -> Result<ChangeHeader, ForgeError> {
    let me = client.viewer()?;
    let data = run(
        client,
        "MergeRequestHeader",
        HEADER,
        json!({ "fullPath": client.project, "iid": number.to_string() }),
    )?;
    let node = merge_request(client, &data)?;
    let summary = summary(client, node, &me).ok_or_else(|| ForgeError::UnexpectedResponse {
        host: client.host.clone(),
        detail: "a merge request without an iid or title".to_string(),
    })?;
    Ok(ChangeHeader {
        summary,
        body: str_at(node, "/description"),
        reviewers: reviewers(node),
        additions: opt_u32(node, "/diffStatsSummary/additions"),
        deletions: opt_u32(node, "/diffStatsSummary/deletions"),
        changed_files: opt_u32(node, "/diffStatsSummary/fileCount"),
        commit_count: opt_u32(node, "/commitCount"),
        timeline: timeline(&array_at(node, "/notes/nodes")),
        timeline_truncated: has_previous_page(node, "/notes"),
    })
}

/// The reviewers with their state, then approvers who were not asked.
fn reviewers(node: &Value) -> Vec<Reviewer> {
    let mut reviewers: Vec<Reviewer> = array_at(node, "/reviewers/nodes")
        .into_iter()
        .filter_map(|reviewer| {
            Some(Reviewer {
                login: opt_str(reviewer, "/username")?.to_string(),
                outcome: mapping::gitlab_reviewer_outcome(opt_str(
                    reviewer,
                    "/mergeRequestInteraction/reviewState",
                )),
            })
        })
        .collect();
    for approver in array_at(node, "/approvedBy/nodes") {
        let Some(login) = opt_str(approver, "/username") else {
            continue;
        };
        match reviewers
            .iter_mut()
            .find(|reviewer| reviewer.login == login)
        {
            Some(existing) => existing.outcome = ReviewOutcome::Approved,
            None => reviewers.push(Reviewer {
                login: login.to_string(),
                outcome: ReviewOutcome::Approved,
            }),
        }
    }
    reviewers
}

/// System notes become events (an approval, a review); a note with a
/// position is a line comment; the rest are comments.
fn timeline(notes: &[&Value]) -> Vec<TimelineItem> {
    notes
        .iter()
        .map(|note| {
            let author = opt_str(note, "/author/username")
                .unwrap_or("ghost")
                .to_string();
            let body = str_at(note, "/body");
            let at = time_at(note, "/createdAt");
            if bool_at(note, "/system") {
                return match mapping::gitlab_system_note(&body) {
                    SystemNote::Approved => TimelineItem::Review {
                        author,
                        outcome: ReviewOutcome::Approved,
                        body: String::new(),
                        at,
                        line_comments: Vec::new(),
                    },
                    SystemNote::Event(kind) => TimelineItem::Event {
                        actor: Some(author),
                        kind,
                        at,
                    },
                };
            }
            match opt_str(note, "/position/filePath") {
                Some(path) => TimelineItem::LineComment(LineComment {
                    author,
                    path: path.to_string(),
                    line: opt_u32(note, "/position/newLine")
                        .or_else(|| opt_u32(note, "/position/oldLine")),
                    body,
                    at,
                }),
                None => TimelineItem::Comment { author, body, at },
            }
        })
        .collect()
}

pub(crate) fn commits(
    client: &ForgeClient,
    number: u64,
) -> Result<Listing<CommitSummary>, ForgeError> {
    paged(|after| {
        let data = plain(
            client,
            "MergeRequestCommits",
            COMMITS,
            json!({ "fullPath": client.project, "iid": number.to_string(), "after": after }),
        )?;
        let merge_request = merge_request(client, &data)?;
        let items = array_at(merge_request, "/commits/nodes")
            .into_iter()
            .filter_map(|commit| {
                Some(CommitSummary {
                    sha: opt_str(commit, "/sha")?.to_string(),
                    short_sha: str_at(commit, "/shortId"),
                    title: str_at(commit, "/title"),
                    author: opt_str(commit, "/author/username")
                        .or_else(|| opt_str(commit, "/authorName"))
                        .unwrap_or("ghost")
                        .to_string(),
                    at: time_at(commit, "/authoredDate"),
                    web_url: str_at(commit, "/webUrl"),
                })
            })
            .collect();
        Ok((items, next_cursor(merge_request, "/commits")))
    })
}

pub(crate) fn checks(client: &ForgeClient, number: u64) -> Result<Listing<Check>, ForgeError> {
    paged(|after| {
        let data = plain(
            client,
            "MergeRequestChecks",
            CHECKS,
            json!({ "fullPath": client.project, "iid": number.to_string(), "after": after }),
        )?;
        let merge_request = merge_request(client, &data)?;
        let items = array_at(merge_request, "/headPipeline/jobs/nodes")
            .into_iter()
            .filter_map(|job| {
                Some(Check {
                    name: opt_str(job, "/name")?.to_string(),
                    status: mapping::gitlab_job(&str_at(job, "/status")),
                    group: opt_str(job, "/stage/name").map(str::to_string),
                    duration_secs: job
                        .get("duration")
                        .and_then(Value::as_f64)
                        .map(|seconds| seconds.max(0.0).round() as u64),
                    url: opt_str(job, "/webPath")
                        .map(|path| format!("https://{}{path}", client.host)),
                })
            })
            .collect();
        Ok((items, next_cursor(merge_request, "/headPipeline/jobs")))
    })
}

/// `diffStats` is a plain list, not a connection: one request, never cut.
pub(crate) fn files(client: &ForgeClient, number: u64) -> Result<Listing<FileChange>, ForgeError> {
    let data = plain(
        client,
        "MergeRequestFiles",
        FILES,
        json!({ "fullPath": client.project, "iid": number.to_string() }),
    )?;
    let items = array_at(merge_request(client, &data)?, "/diffStats")
        .into_iter()
        .filter_map(|file| {
            Some(FileChange {
                path: opt_str(file, "/path")?.to_string(),
                kind: None,
                additions: u32_at(file, "/additions"),
                deletions: u32_at(file, "/deletions"),
            })
        })
        .collect();
    Ok(Listing {
        items,
        truncated: false,
    })
}

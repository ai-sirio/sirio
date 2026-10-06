//! GitLab's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/gitlab/` was validated against gitlab.com on
//! 2026-09-27 (`curl https://gitlab.com/api/graphql`, project
//! gitlab-org/cli); the header query's `diffRefs` was added and run there
//! again on 2026-09-28 (merge request !2000, full and baseline variants).
//! Change a field only after running the changed query there again.
//!
//! GraphQL rejects a whole query that names a field the server lacks, and
//! self-managed installations run old versions. So the queries that carry
//! a merge request summary have a baseline variant without the newest
//! fields: the first `Field '…' doesn't exist` switches the client to the
//! baseline for its lifetime (spec §6.3).

use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::action::{
    Action, ActionContext, ActionOutcome, LiveProbe, RerunTarget, ReviewTarget, ReviewVerdict, ThreadFacts, check_action,
};
use crate::client::{ForgeClient, page, paged, percent_encode, pick_for_branch};
use crate::error::ForgeError;
use crate::graphql::{
    array_at, bool_at, execute, execute_mutation, execute_rest, has_previous_page, interpret_rest, next_cursor,
    no_unknown_field, opt_bool, opt_str, opt_u32, revisions, str_at, time_at, u32_at,
};
use crate::mapping::{self, SystemNote};
use crate::model::{
    Candidate, Capabilities, Label, MergeMethod, ChangeHeader, ChangePage, ChangeSummary, Check, CheckJob, Log, CommentKind, CommentRef,
    CommitSummary, FileChange, Filter, LineComment, ListQuery, Listing, PageCursor,
    ReviewOutcome, ReviewThread, Reviewer, ThreadComment, TimelineItem,
};
use crate::scopes::TokenScopes;
use crate::transport::{RestMethod, RestRequest};

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
const ACTION_CONTEXT: (&str, &str) = (
    include_str!("queries/gitlab/action_context.graphql"),
    include_str!("queries/gitlab/action_context_baseline.graphql"),
);
const ACCEPT: &str = include_str!("queries/gitlab/accept.graphql");
const SET_LABELS: &str = include_str!("queries/gitlab/set_labels.graphql");
const SET_REVIEWERS: &str = include_str!("queries/gitlab/set_reviewers.graphql");
const REVIEWER_CANDIDATES: &str = include_str!("queries/gitlab/reviewer_candidates.graphql");
const LABEL_CANDIDATES: &str = include_str!("queries/gitlab/label_candidates.graphql");
const CREATE_NOTE: &str = include_str!("queries/gitlab/create_note.graphql");
const TOGGLE_RESOLVE: &str = include_str!("queries/gitlab/toggle_resolve.graphql");
const UPDATE_NOTE: &str = include_str!("queries/gitlab/update_note.graphql");
const UPDATE: &str = include_str!("queries/gitlab/update.graphql");
const SET_DRAFT: &str = include_str!("queries/gitlab/set_draft.graphql");
const PIPELINE_RETRY: &str = include_str!("queries/gitlab/pipeline_retry.graphql");
const JOB_RETRY: &str = include_str!("queries/gitlab/job_retry.graphql");
const REQUEST_CHANGES: &str = include_str!("queries/gitlab/request_changes.graphql");
const COUNT: &str = include_str!("queries/gitlab/count.graphql");
const COMMITS: &str = include_str!("queries/gitlab/commits.graphql");
const CHECKS: &str = include_str!("queries/gitlab/checks.graphql");
const FILES: &str = include_str!("queries/gitlab/files.graphql");
const THREADS: &str = include_str!("queries/gitlab/threads.graphql");
const THREADS_BASELINE: &str = include_str!("queries/gitlab/threads_baseline.graphql");

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

/// The viewer's draft notes (REST: GraphQL has none, checked against the
/// schema on 2026-10-05). A server or role without draft notes answers
/// `NotFound` or `Forbidden`, which reads as none (B3c revision (h)).
fn draft_notes(client: &ForgeClient, number: u64) -> Result<Vec<Value>, ForgeError> {
    let path = format!(
        "projects/{}/merge_requests/{number}/draft_notes?per_page=100",
        percent_encode(&client.project, false)
    );
    match execute_rest(client, &RestRequest::get(path)) {
        Ok(response) => Ok(serde_json::from_slice::<Vec<Value>>(&response.body).map_err(|error| {
            ForgeError::UnexpectedResponse { host: client.host.clone(), detail: format!("draft notes: {error}") }
        })?),
        Err(ForgeError::NotFound { .. } | ForgeError::Forbidden { .. }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
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
        capabilities: capabilities(client, node),
        body: str_at(node, "/description"),
        reviewers: reviewers(node),
        labels: labels(node),
        additions: opt_u32(node, "/diffStatsSummary/additions"),
        deletions: opt_u32(node, "/diffStatsSummary/deletions"),
        changed_files: opt_u32(node, "/diffStatsSummary/fileCount"),
        commit_count: opt_u32(node, "/commitCount"),
        timeline: timeline(&array_at(node, "/notes/nodes")),
        timeline_truncated: has_previous_page(node, "/notes"),
        draft: mapping::gitlab_draft(&draft_notes(client, number)?),
        revisions: revisions(
            opt_str(node, "/diffRefs/baseSha"),
            opt_str(node, "/diffRefs/headSha"),
            opt_str(node, "/diffRefs/startSha"),
        ),
    })
}

/// What the viewer may do. A baseline query leaves the fields an old server
/// lacks unasked, and the mapping reads "not reported" as "not offered".
fn capabilities(client: &ForgeClient, node: &Value) -> Capabilities {
    let strategies: Option<Vec<&str>> = node
        .pointer("/availableAutoMergeStrategies")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect());
    let merge = mapping::gitlab_merge(mapping::GitLabMergeFacts {
        state: opt_str(node, "/state").unwrap_or(""),
        detailed_status: opt_str(node, "/detailedMergeStatus"),
        can_merge: opt_bool(node, "/userPermissions/canMerge"),
        squash_read_only: opt_bool(node, "/squashReadOnly"),
        squash_on_merge: opt_bool(node, "/squashOnMerge"),
        auto_merge_enabled: opt_bool(node, "/autoMergeEnabled"),
        auto_merge_strategies: strategies.as_deref(),
        remove_source_branch: opt_bool(node, "/shouldRemoveSourceBranch"),
    });
    let mut caps = mapping::gitlab_capabilities(mapping::GitLabFacts {
        state: opt_str(node, "/state").unwrap_or(""),
        locked: bool_at(node, "/discussionLocked"),
        can_create_note: opt_bool(node, "/userPermissions/createNote"),
        can_update: opt_bool(node, "/userPermissions/updateMergeRequest"),
        can_approve: opt_bool(node, "/userPermissions/canApprove"),
        reports_review_state: !client.baseline.load(Ordering::Relaxed),
        can_update_pipeline: opt_bool(node, "/headPipeline/userPermissions/updatePipeline"),
    });
    caps.merge = merge;
    caps
}

fn labels(node: &Value) -> Vec<Label> {
    array_at(node, "/labels/nodes")
        .into_iter()
        .filter_map(|label| {
            Some(Label {
                id: opt_str(label, "/id")?.to_string(),
                name: opt_str(label, "/title")?.to_string(),
                color: opt_str(label, "/color").map(str::to_string),
            })
        })
        .collect()
}

/// An edit handle for a note the viewer may administer.
fn note_edit(note: &Value) -> Option<CommentRef> {
    if !bool_at(note, "/userPermissions/adminNote") {
        return None;
    }
    opt_str(note, "/id").map(|id| CommentRef {
        id: id.to_string(),
        kind: CommentKind::Comment,
    })
}

/// The reviewers with their state, then approvers who were not asked.
fn reviewers(node: &Value) -> Vec<Reviewer> {
    let mut reviewers: Vec<Reviewer> = array_at(node, "/reviewers/nodes")
        .into_iter()
        .filter_map(|reviewer| {
            let login = opt_str(reviewer, "/username")?.to_string();
            Some(Reviewer {
                id: Some(login.clone()),
                login,
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
                id: Some(login.to_string()),
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
                        edit: None,
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
                None => TimelineItem::Comment {
                    author,
                    body,
                    at,
                    edit: note_edit(note),
                },
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
        let pipeline = opt_str(merge_request, "/headPipeline/id").and_then(mapping::gitlab_gid_number);
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
                    job: opt_str(job, "/id")
                        .and_then(mapping::gitlab_gid_number)
                        .map(|job_id| CheckJob {
                            job_id,
                            run_id: pipeline,
                            retryable: opt_bool(job, "/retryable") == Some(true),
                        }),
                })
            })
            .collect();
        Ok((items, next_cursor(merge_request, "/headPipeline/jobs")))
    })
}

pub(crate) fn job_log(client: &ForgeClient, job: &CheckJob) -> Result<Log, ForgeError> {
    let project = percent_encode(&client.project, false);
    let status = execute_rest(client, &RestRequest::get(format!("projects/{project}/jobs/{}", job.job_id)))?;
    let complete = serde_json::from_slice::<Value>(&status.body)
        .ok()
        .and_then(|answer| answer.get("status").and_then(Value::as_str).map(mapping::gitlab_job_settled))
        .unwrap_or(false);
    let mut request = RestRequest::get(format!("projects/{project}/jobs/{}/trace", job.job_id));
    request.log = true;
    let response = client.transport.request(&request)?;
    let bytes = match response.status {
        200..=299 => response.body,
        // An archived trace may live in object storage.
        301 | 302 | 303 | 307 | 308 => {
            let location = response.header("location").ok_or_else(|| ForgeError::UnexpectedResponse {
                host: client.host.clone(),
                detail: "a redirect with no location".to_string(),
            })?;
            crate::log::fetch_signed(&client.host, location)?
        }
        404 if !complete => return Ok(crate::log::finish(Vec::new(), false, false)),
        _ => return Err(interpret_rest(&client.host, response).err().unwrap_or(ForgeError::UnexpectedResponse {
            host: client.host.clone(),
            detail: "an unreadable log answer".to_string(),
        })),
    };
    // GitLab serves a running job's trace as it grows; a job that never
    // started answers an empty one.
    let published = !bytes.is_empty();
    Ok(crate::log::finish(bytes, complete, published))
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

/// Diff discussions as threads (spec §4). A server without
/// `truncatedDiffLines` gets the query without it, for this read only: the
/// client's shared baseline flag is left alone, so the header keeps its own
/// newer fields.
pub(crate) fn review_threads(client: &ForgeClient, number: u64) -> Result<Listing<ReviewThread>, ForgeError> {
    let mut text = THREADS;
    let mut head: Option<String> = None;
    let mut listing = paged(|after| {
        let variables = json!({ "fullPath": client.project, "iid": number.to_string(), "after": after });
        let data = match execute(client, "MergeRequestThreads", text, variables.clone()) {
            Err(ForgeError::UnknownField { .. }) if text == THREADS => {
                text = THREADS_BASELINE;
                plain(client, "MergeRequestThreads", text, variables)?
            }
            answer => answer.map_err(no_unknown_field)?,
        };
        let merge_request = merge_request(client, &data)?;
        if head.is_none() {
            head = opt_str(merge_request, "/diffRefs/headSha").map(str::to_string);
        }
        let can_note = bool_at(merge_request, "/userPermissions/createNote");
        let items = array_at(merge_request, "/discussions/nodes")
            .into_iter()
            .filter_map(|discussion| gitlab_thread(discussion, head.as_deref(), can_note))
            .collect();
        Ok((items, next_cursor(merge_request, "/discussions")))
    })?;
    let me = client.viewer()?;
    mapping::gitlab_drafts_into(&mut listing.items, &draft_notes(client, number)?, &me, head.as_deref());
    Ok(listing)
}

fn gitlab_thread(discussion: &Value, head: Option<&str>, can_note: bool) -> Option<ReviewThread> {
    let notes = array_at(discussion, "/notes/nodes");
    let first = notes.first()?;
    // A system note or a note with no position is not a diff thread.
    if bool_at(first, "/system") {
        return None;
    }
    let path = opt_str(first, "/position/filePath")?.to_string();
    let (side, line) = mapping::gitlab_anchor(
        opt_u32(first, "/position/newLine"),
        opt_u32(first, "/position/oldLine"),
    );
    let quoted: Vec<&str> = array_at(discussion, "/truncatedDiffLines")
        .into_iter()
        .filter_map(|line| opt_str(line, "/text"))
        .collect();
    let resolved = bool_at(discussion, "/resolved");
    Some(ReviewThread {
        id: opt_str(discussion, "/id")?.to_string(),
        path,
        side,
        line,
        start_line: None,
        outdated: mapping::gitlab_thread_outdated(opt_str(first, "/position/diffRefs/headSha"), head),
        resolved,
        resolved_by: opt_str(discussion, "/resolvedBy/username").map(str::to_string),
        diff_hunk: (!quoted.is_empty()).then(|| quoted.join("\n")),
        can_reply: can_note,
        can_resolve: bool_at(discussion, "/resolvable") && bool_at(discussion, "/userPermissions/resolveNote"),
        file_level: matches!(opt_str(first, "/position/positionType"), Some("file" | "image")),
        comments: notes
            .iter()
            .filter(|note| !bool_at(note, "/system"))
            .filter_map(|note| {
                Some(ThreadComment {
                    id: opt_str(note, "/id")?.to_string(),
                    author: opt_str(note, "/author/username").unwrap_or("ghost").to_string(),
                    body: str_at(note, "/body"),
                    at: time_at(note, "/createdAt"),
                    edit: note_edit(note),
                    pending: false,
                })
            })
            .collect(),
    })
}

/// The pre-flight read of one merge request: its global id (what a note is
/// attached to), its state and what the viewer may do to it now. On a server
/// that lacks `canApprove` the baseline query is used, and approving reads
/// as "not reported".
fn action_context(client: &ForgeClient, number: u64) -> Result<ActionContext, ForgeError> {
    let data = run(
        client,
        "MergeRequestActionContext",
        ACTION_CONTEXT,
        json!({ "fullPath": client.project, "iid": number.to_string() }),
    )?;
    let node = merge_request(client, &data)?;
    let node_id = opt_str(node, "/id")
        .ok_or_else(|| ForgeError::UnexpectedResponse {
            host: client.host.clone(),
            detail: "a merge request without an id".to_string(),
        })?
        .to_string();
    let state = mapping::gitlab_change_state(&str_at(node, "/state"), bool_at(node, "/draft"));
    Ok(ActionContext {
        node_id,
        state,
        head_sha: opt_str(node, "/diffHeadSha").map(str::to_string),
        head_ref_name: None,
        cross_repository: false,
        reviewer_ids: array_at(node, "/reviewers/nodes")
            .into_iter()
            .filter_map(|reviewer| opt_str(reviewer, "/username").map(str::to_string))
            .collect(),
        team_ids: Vec::new(),
        bot_ids: Vec::new(),
        unsendable_requests: Vec::new(),
        capabilities: capabilities(client, node),
        thread: None,
        draft: None,
    })
}

fn mutate(
    client: &ForgeClient,
    operation: &str,
    document: &str,
    input: Value,
) -> Result<(), ForgeError> {
    execute_mutation(client, operation, document, json!({ "input": input })).map(|_| ())
}

/// One discussion's permissions, read afresh from the discussions list:
/// GitLab's GraphQL has no discussion by id. A reply needs the merge
/// request's `createNote`; resolving and reopening need `resolveNote` on a
/// resolvable discussion.
fn thread_facts(client: &ForgeClient, number: u64, id: &str) -> Result<ThreadFacts, ForgeError> {
    let listing = review_threads(client, number)?;
    let thread = listing
        .items
        .iter()
        .find(|thread| thread.id == id)
        .ok_or_else(|| ForgeError::NotFound { host: client.host.clone() })?;
    Ok(ThreadFacts {
        can_reply: thread.can_reply,
        can_resolve: thread.can_resolve,
        can_unresolve: thread.can_resolve,
    })
}

fn create_note(client: &ForgeClient, noteable: &str, body: &str) -> Result<(), ForgeError> {
    mutate(
        client,
        "CreateNote",
        CREATE_NOTE,
        json!({ "noteableId": noteable, "body": body }),
    )
}

/// GitLab has no approve mutation in GraphQL (checked against gitlab.com on
/// 2026-09-29): approving is the REST call.
fn approve(client: &ForgeClient, number: u64) -> Result<(), ForgeError> {
    let request = RestRequest {
        method: RestMethod::Post,
        log: false,
        path: format!(
            "projects/{}/merge_requests/{number}/approve",
            percent_encode(&client.project, false)
        ),
        body: None,
    };
    execute_rest(client, &request).map(|_| ())
}

/// Requesting changes shares one call between the `Review` action and the
/// review submit below.
fn request_changes(client: &ForgeClient, iid: &str) -> Result<(), ForgeError> {
    mutate(
        client,
        "MergeRequestRequestChanges",
        REQUEST_CHANGES,
        json!({ "projectPath": client.project, "iid": iid }),
    )
}

/// `mergeRequestUpdate` with only the fields the action changes.
fn update(client: &ForgeClient, number: u64, fields: Value) -> Result<(), ForgeError> {
    let mut input = json!({ "projectPath": client.project, "iid": number.to_string() });
    if let (Some(input), Some(fields)) = (input.as_object_mut(), fields.as_object()) {
        input.extend(fields.clone());
    }
    mutate(client, "MergeRequestUpdate", UPDATE, input)
}

/// The action's first step decided it; a second step (the comment beside an
/// approval) that fails leaves the first standing, and says so.
fn then_comment(
    client: &ForgeClient,
    noteable: &str,
    body: &str,
    done: &str,
) -> ActionOutcome {
    if body.trim().is_empty() {
        return ActionOutcome::default();
    }
    match create_note(client, noteable, body) {
        Ok(()) => ActionOutcome::default(),
        Err(error) => ActionOutcome {
            warning: Some(format!("{done}, but the comment could not be posted: {error}")),
        },
    }
}

pub(crate) fn act(
    client: &ForgeClient,
    number: u64,
    action: &Action,
) -> Result<ActionOutcome, ForgeError> {
    let mut context = action_context(client, number)?;
    if let Action::Reply { thread, .. }
    | Action::Resolve { thread, .. }
    | Action::ReviewAdd { target: ReviewTarget::Reply { thread }, .. } = action
    {
        context.thread = Some(thread_facts(client, number, thread)?);
    }
    let mut drafts = Vec::new();
    if matches!(action, Action::Review { .. } | Action::ReviewSubmit { .. } | Action::ReviewDiscard) {
        drafts = draft_notes(client, number)?;
        context.draft = mapping::gitlab_draft(&drafts);
    }
    check_action(&client.host, action, &context)?;
    let noteable = context.node_id.as_str();
    let iid = number.to_string();
    let drafts_path = format!("projects/{}/merge_requests/{number}/draft_notes", percent_encode(&client.project, false));
    match action {
        Action::Comment { body }
        | Action::Review {
            verdict: ReviewVerdict::Comment,
            body,
        } => create_note(client, noteable, body)?,
        Action::Review {
            verdict: ReviewVerdict::Approve,
            body,
        } => {
            approve(client, number)?;
            return Ok(then_comment(client, noteable, body, "Approved"));
        }
        Action::Review {
            verdict: ReviewVerdict::RequestChanges,
            body,
        } => {
            request_changes(client, &iid)?;
            return Ok(then_comment(client, noteable, body, "Changes requested"));
        }
        Action::Close => update(client, number, json!({ "state": "CLOSED" }))?,
        Action::Reopen => update(client, number, json!({ "state": "OPEN" }))?,
        Action::MarkReady => mutate(
            client,
            "MergeRequestSetDraft",
            SET_DRAFT,
            json!({ "projectPath": client.project, "iid": iid, "draft": false }),
        )?,
        Action::ConvertToDraft => mutate(
            client,
            "MergeRequestSetDraft",
            SET_DRAFT,
            json!({ "projectPath": client.project, "iid": iid, "draft": true }),
        )?,
        Action::Edit {
            title,
            body,
            target_branch,
        } => {
            let mut fields = json!({});
            if let Some(title) = title {
                fields["title"] = json!(title);
            }
            if let Some(body) = body {
                fields["description"] = json!(body);
            }
            if let Some(branch) = target_branch {
                fields["targetBranch"] = json!(branch);
            }
            update(client, number, fields)?
        }
        Action::Merge {
            method,
            commit_title,
            commit_message,
            delete_branch,
            when_checks_pass,
            expected_head,
        } => {
            let squash = match method {
                MergeMethod::Merge => false,
                MergeMethod::Squash => true,
                MergeMethod::Rebase => {
                    return Err(ForgeError::Unsupported {
                        host: client.host.clone(),
                        what: "rebase merge".to_string(),
                    });
                }
            };
            let mut input = json!({
                "projectPath": client.project,
                "iid": iid,
                "sha": expected_head,
                "squash": squash,
                "shouldRemoveSourceBranch": delete_branch,
            });
            let message = commit_message.as_deref().filter(|m| !m.trim().is_empty());
            let text = match (commit_title, message) {
                (Some(title), Some(message)) => Some(format!("{title}\n\n{message}")),
                (Some(title), None) => Some(title.clone()),
                (None, Some(message)) => Some(message.to_string()),
                (None, None) => None,
            };
            if let Some(text) = text {
                input[if squash { "squashCommitMessage" } else { "commitMessage" }] = json!(text);
            }
            if *when_checks_pass {
                input["strategy"] = json!("MERGE_WHEN_CHECKS_PASS");
            }
            newer_write(client, "MergeRequestAccept", ACCEPT, input)?
        }
        // GraphQL has no way to cancel an auto-merge (checked against
        // gitlab.com on 2026-10-03).
        Action::CancelAutoMerge => execute_rest(
            client,
            &RestRequest {
                method: RestMethod::Post,
                log: false,
                path: format!(
                    "projects/{}/merge_requests/{number}/cancel_merge_when_pipeline_succeeds",
                    percent_encode(&client.project, false)
                ),
                body: None,
            },
        )
        .map(|_| ())?,
        Action::SetReviewers { add, remove } => {
            return set_each(client, "MergeRequestSetReviewers", SET_REVIEWERS, "reviewerUsernames", &iid, add, remove);
        }
        Action::SetLabels { add, remove } => {
            return set_each(client, "MergeRequestSetLabels", SET_LABELS, "labelIds", &iid, add, remove);
        }
        Action::Rerun(target) => {
            let (operation, document, id) = match target {
                RerunTarget::FailedInRun(pipeline) => (
                    "PipelineRetry",
                    PIPELINE_RETRY,
                    format!("gid://gitlab/Ci::Pipeline/{pipeline}"),
                ),
                RerunTarget::Job(job) => ("JobRetry", JOB_RETRY, format!("gid://gitlab/Ci::Build/{job}")),
            };
            mutate(client, operation, document, json!({ "id": id }))?;
        }
        Action::Reply { thread, body } => mutate(
            client,
            "CreateNote",
            CREATE_NOTE,
            json!({ "noteableId": noteable, "discussionId": thread, "body": body }),
        )?,
        Action::Resolve { thread, resolved } => mutate(
            client,
            "DiscussionToggleResolve",
            TOGGLE_RESOLVE,
            json!({ "id": thread, "resolve": resolved }),
        )?,
        // GraphQL's `DiffPositionInput` has no line range (checked against
        // gitlab.com on 2026-10-05): every line comment takes REST, one
        // path for a line and a range.
        Action::LineComment { anchor, revisions, body } => {
            let position = mapping::gitlab_position(anchor, revisions);
            execute_rest(
                client,
                &RestRequest {
                    method: RestMethod::Post,
                    log: false,
                    path: format!(
                        "projects/{}/merge_requests/{number}/discussions",
                        percent_encode(&client.project, false)
                    ),
                    body: Some(json!({ "body": body, "position": position }).to_string().into_bytes()),
                },
            )
            .map(|_| ())?
        }
        Action::EditComment { comment, body } if comment.kind == CommentKind::Draft => {
            execute_rest(client, &RestRequest {
                method: RestMethod::Put,
                log: false,
                path: format!("{drafts_path}/{}", comment.id),
                body: Some(json!({ "note": body }).to_string().into_bytes()),
            })
            .map(|_| ())?
        }
        Action::EditComment { comment, body } => mutate(
            client,
            "UpdateNote",
            UPDATE_NOTE,
            json!({ "id": comment.id, "body": body }),
        )?,
        Action::ReviewAdd { target, body } => {
            execute_rest(client, &RestRequest {
                method: RestMethod::Post,
                log: false,
                path: drafts_path.clone(),
                body: Some(mapping::gitlab_draft_note(target, body).to_string().into_bytes()),
            })
            .map(|_| ())?
        }
        // Two steps (spec §5): once the drafts are published, a verdict that
        // fails is said as a warning, never as an error that would read as if
        // nothing had been published.
        Action::ReviewSubmit { verdict, body } => {
            execute_rest(client, &RestRequest {
                method: RestMethod::Post,
                log: false,
                path: format!("{drafts_path}/bulk_publish"),
                body: None,
            })?;
            let published = "Your review was published";
            let verdict_failed = |what: &str, error: ForgeError| {
                let mut warning = format!("{published}, but {what} failed: {error}");
                if !body.trim().is_empty() {
                    if let Err(note_error) = create_note(client, noteable, body) {
                        warning.push_str(&format!("; your summary was not posted either: {note_error}"));
                    }
                }
                ActionOutcome { warning: Some(warning) }
            };
            return Ok(match verdict {
                ReviewVerdict::Comment => then_comment(client, noteable, body, published),
                ReviewVerdict::Approve => match approve(client, number) {
                    Ok(()) => then_comment(client, noteable, body, "Your review was published and approved"),
                    Err(error) => verdict_failed("approving", error),
                },
                ReviewVerdict::RequestChanges => match request_changes(client, &iid) {
                    Ok(()) => then_comment(client, noteable, body, "Your review was published with changes requested"),
                    Err(error) => verdict_failed("requesting changes", error),
                },
            });
        }
        Action::ReviewDiscard => {
            for note in &drafts {
                if let Some(id) = note.get("id").and_then(Value::as_u64) {
                    execute_rest(client, &RestRequest { method: RestMethod::Delete, log: false, path: format!("{drafts_path}/{id}"), body: None })?;
                }
            }
        }
        Action::DraftDelete { comment } => {
            execute_rest(client, &RestRequest { method: RestMethod::Delete, log: false, path: format!("{drafts_path}/{}", comment.id), body: None })
                .map(|_| ())?
        }
    }
    Ok(ActionOutcome::default())
}

/// A field or mutation this server does not have: the action is not
/// available here, which is not a malformed answer.
fn unsupported(client: &ForgeClient, what: &str) -> impl Fn(ForgeError) -> ForgeError {
    let (host, what) = (client.host.clone(), what.to_string());
    move |error| match error {
        ForgeError::UnknownField { .. } => ForgeError::Unsupported {
            host: host.clone(),
            what: what.clone(),
        },
        other => other,
    }
}

/// A write only newer GitLab servers have.
fn newer_write(client: &ForgeClient, operation: &str, document: &str, input: Value) -> Result<(), ForgeError> {
    execute_mutation(client, operation, document, json!({ "input": input }))
        .map(|_| ())
        .map_err(unsupported(client, operation))
}

/// `APPEND` what was added, then `REMOVE` what was taken away; an empty
/// side is not sent. A failed removal after an addition went through
/// leaves the addition standing, and says so.
fn set_each(
    client: &ForgeClient,
    operation: &str,
    document: &str,
    key: &str,
    iid: &str,
    add: &[String],
    remove: &[String],
) -> Result<ActionOutcome, ForgeError> {
    let send = |mode: &str, ids: &[String]| {
        let mut input = json!({ "projectPath": client.project, "iid": iid, "operationMode": mode });
        input[key] = json!(ids);
        newer_write(client, operation, document, input)
    };
    if !add.is_empty() {
        send("APPEND", add)?;
    }
    if !remove.is_empty() {
        match send("REMOVE", remove) {
            Ok(()) => {}
            Err(error) if !add.is_empty() => {
                return Ok(ActionOutcome {
                    warning: Some(format!("added, but removing the others failed: {error}")),
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(ActionOutcome::default())
}

pub(crate) fn reviewer_candidates(client: &ForgeClient, text: &str) -> Result<Vec<Candidate>, ForgeError> {
    let data = execute(
        client,
        "ReviewerCandidates",
        REVIEWER_CANDIDATES,
        json!({ "fullPath": client.project, "q": text }),
    )
    .map_err(unsupported(client, "reviewer search"))?;
    Ok(array_at(&data, "/project/projectMembers/nodes")
        .into_iter()
        .filter_map(|member| {
            let username = opt_str(member, "/user/username")?;
            Some(Candidate {
                id: username.to_string(),
                label: username.to_string(),
                note: opt_str(member, "/user/name").filter(|name| !name.is_empty()).map(str::to_string),
            })
        })
        .collect())
}

pub(crate) fn label_candidates(client: &ForgeClient, text: &str) -> Result<Vec<Candidate>, ForgeError> {
    let data = execute(
        client,
        "LabelCandidates",
        LABEL_CANDIDATES,
        json!({ "fullPath": client.project, "q": text }),
    )
    .map_err(unsupported(client, "label search"))?;
    Ok(labels(data.pointer("/project").unwrap_or(&Value::Null))
        .into_iter()
        .map(|label| Candidate {
            id: label.id,
            label: label.name,
            note: None,
        })
        .collect())
}

/// A personal access token describes itself at `personal_access_tokens/self`.
pub(crate) fn token_scopes(client: &ForgeClient) -> Option<TokenScopes> {
    let request = RestRequest {
        method: RestMethod::Get,
        log: false,
        path: "personal_access_tokens/self".to_string(),
        body: None,
    };
    let response = execute_rest(client, &request).ok()?;
    let value: Value = serde_json::from_slice(&response.body).ok()?;
    let scopes = value.get("scopes")?.as_array()?;
    Some(TokenScopes(
        scopes
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    ))
}

/// See [`crate::action::live_probes`]. GitLab's iids start at 1 and its ids at
/// 1, so `0` names nothing, in any project.
pub(crate) fn live_probes() -> Vec<LiveProbe> {
    let project = "gitlab-org/cli";
    let write = |operation, document, input: Value| LiveProbe {
        operation,
        document,
        variables: json!({ "input": input }),
    };
    vec![
        LiveProbe {
            operation: "MergeRequestActionContext",
            document: ACTION_CONTEXT.0,
            variables: json!({ "fullPath": project, "iid": "2000" }),
        },
        LiveProbe {
            operation: "MergeRequestActionContext",
            document: ACTION_CONTEXT.1,
            variables: json!({ "fullPath": project, "iid": "2000" }),
        },
        write(
            "CreateNote",
            CREATE_NOTE,
            json!({ "noteableId": "gid://gitlab/MergeRequest/0", "discussionId": "gid://gitlab/Discussion/0", "body": "x" }),
        ),
        write(
            "DiscussionToggleResolve",
            TOGGLE_RESOLVE,
            json!({ "id": "gid://gitlab/Discussion/0", "resolve": true }),
        ),
        write(
            "UpdateNote",
            UPDATE_NOTE,
            json!({ "id": "gid://gitlab/Note/0", "body": "x" }),
        ),
        write(
            "MergeRequestUpdate",
            UPDATE,
            json!({ "projectPath": project, "iid": "0", "title": "x", "description": "x", "targetBranch": "x", "state": "OPEN" }),
        ),
        write(
            "MergeRequestSetDraft",
            SET_DRAFT,
            json!({ "projectPath": project, "iid": "0", "draft": true }),
        ),
        write(
            "MergeRequestRequestChanges",
            REQUEST_CHANGES,
            json!({ "projectPath": project, "iid": "0" }),
        ),
        write(
            "MergeRequestAccept",
            ACCEPT,
            json!({ "projectPath": project, "iid": "0", "sha": "0", "squash": false,
                    "shouldRemoveSourceBranch": false, "commitMessage": "x",
                    "strategy": "MERGE_WHEN_CHECKS_PASS" }),
        ),
        write(
            "MergeRequestSetLabels",
            SET_LABELS,
            json!({ "projectPath": project, "iid": "0", "labelIds": ["gid://gitlab/ProjectLabel/0"], "operationMode": "APPEND" }),
        ),
        write(
            "MergeRequestSetReviewers",
            SET_REVIEWERS,
            json!({ "projectPath": project, "iid": "0", "reviewerUsernames": ["sirio-live-check-0"], "operationMode": "APPEND" }),
        ),
        LiveProbe {
            operation: "ReviewerCandidates",
            document: REVIEWER_CANDIDATES,
            variables: json!({ "fullPath": project, "q": "a" }),
        },
        LiveProbe {
            operation: "LabelCandidates",
            document: LABEL_CANDIDATES,
            variables: json!({ "fullPath": project, "q": "bug" }),
        },
        LiveProbe {
            operation: "PipelineRetry",
            document: PIPELINE_RETRY,
            variables: json!({ "input": { "id": "gid://gitlab/Ci::Pipeline/0" } }),
        },
        LiveProbe {
            operation: "JobRetry",
            document: JOB_RETRY,
            variables: json!({ "input": { "id": "gid://gitlab/Ci::Build/0" } }),
        },
    ]
}

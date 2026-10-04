//! GitHub's GraphQL: the queries Sirio sends and what it reads back.
//!
//! Every query in `queries/github/` was validated against github.com on
//! 2026-09-27 (`gh api graphql`, repository ai-sirio/sirio); the header
//! query's `baseRefOid`/`headRefOid` fields were added and run there again
//! on 2026-09-28 (pull request #578). Change a field only after running the
//! changed query there again.

use serde_json::{Value, json};

use crate::action::{
    Action, ActionContext, ActionOutcome, LiveProbe, RerunTarget, ReviewVerdict, check_action,
};
use crate::client::{ForgeClient, page, paged, percent_encode, pick_for_branch};
use crate::error::ForgeError;
use crate::graphql::{
    array_at, bool_at, execute, execute_mutation, execute_rest, has_previous_page, next_cursor,
    no_unknown_field, opt_str, opt_u32, revisions, str_at, time_at, u32_at,
};
use crate::mapping;
use crate::scopes::TokenScopes;
use crate::transport::{RestMethod, RestRequest};
use crate::model::{
    Candidate, Capabilities, ChangeHeader, ChangeState, Label, MergeCapability, MergeMethod, ChangePage, ChangeSummary, Check, CheckJob, CiState, CommentKind,
    CommentRef, CommitSummary, EventKind, FileChange, Filter, LineComment, ListQuery, Listing,
    PageCursor, ReviewOutcome, Reviewer, TimelineItem,
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
const ACTION_CONTEXT: &str = include_str!("queries/github/action_context.graphql");
const ADD_COMMENT: &str = include_str!("queries/github/add_comment.graphql");
const ADD_REVIEW: &str = include_str!("queries/github/add_review.graphql");
const CLOSE: &str = include_str!("queries/github/close.graphql");
const REOPEN: &str = include_str!("queries/github/reopen.graphql");
const READY: &str = include_str!("queries/github/ready.graphql");
const DRAFT: &str = include_str!("queries/github/draft.graphql");
const UPDATE: &str = include_str!("queries/github/update.graphql");
const UPDATE_COMMENT: &str = include_str!("queries/github/update_comment.graphql");
const UPDATE_REVIEW: &str = include_str!("queries/github/update_review.graphql");
const MERGE: &str = include_str!("queries/github/merge.graphql");
const ENABLE_AUTO_MERGE: &str = include_str!("queries/github/enable_auto_merge.graphql");
const DISABLE_AUTO_MERGE: &str = include_str!("queries/github/disable_auto_merge.graphql");
const REQUEST_REVIEWS: &str = include_str!("queries/github/request_reviews.graphql");
const ADD_LABELS: &str = include_str!("queries/github/add_labels.graphql");
const REMOVE_LABELS: &str = include_str!("queries/github/remove_labels.graphql");
const REVIEWER_CANDIDATES: &str = include_str!("queries/github/reviewer_candidates.graphql");
const LABEL_CANDIDATES: &str = include_str!("queries/github/label_candidates.graphql");

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
    let repository = data.pointer("/repository").unwrap_or(&Value::Null);
    Ok(ChangeHeader {
        summary,
        capabilities: capabilities(node, repository),
        body: str_at(node, "/body"),
        reviewers,
        labels: labels(node),
        additions: opt_u32(node, "/additions"),
        deletions: opt_u32(node, "/deletions"),
        changed_files: opt_u32(node, "/changedFiles"),
        commit_count: opt_u32(node, "/allCommits/totalCount"),
        timeline_truncated: has_previous_page(node, "/timelineItems"),
        timeline,
        revisions: revisions(opt_str(node, "/baseRefOid"), opt_str(node, "/headRefOid"), None),
    })
}

fn labels(node: &Value) -> Vec<Label> {
    array_at(node, "/labels/nodes")
        .into_iter()
        .filter_map(|label| {
            Some(Label {
                id: opt_str(label, "/id")?.to_string(),
                name: opt_str(label, "/name")?.to_string(),
                color: opt_str(label, "/color").map(str::to_string),
            })
        })
        .collect()
}

/// The merge strip's facts: the pull request's merge state beside the
/// repository's merge settings.
fn merge_capability(node: &Value, repository: &Value) -> MergeCapability {
    let state = mapping::github_change_state(&str_at(node, "/state"), bool_at(node, "/isDraft"));
    let state_word = match state {
        ChangeState::Open => "OPEN",
        ChangeState::Draft => "DRAFT",
        ChangeState::Closed => "CLOSED",
        ChangeState::Merged => "MERGED",
    };
    let rollup = array_at(node, "/commits/nodes")
        .last()
        .and_then(|commit| opt_str(commit, "/commit/statusCheckRollup/state"));
    mapping::github_merge(mapping::GitHubMergeFacts {
        state: state_word,
        merge_state_status: opt_str(node, "/mergeStateStatus"),
        review_decision: opt_str(node, "/reviewDecision"),
        rollup,
        merge_commit_allowed: bool_at(repository, "/mergeCommitAllowed"),
        squash_merge_allowed: bool_at(repository, "/squashMergeAllowed"),
        rebase_merge_allowed: bool_at(repository, "/rebaseMergeAllowed"),
        auto_merge_allowed: bool_at(repository, "/autoMergeAllowed"),
        viewer_can_enable_auto_merge: bool_at(node, "/viewerCanEnableAutoMerge"),
        auto_merge_method: opt_str(node, "/autoMergeRequest/mergeMethod"),
        delete_branch_on_merge: bool_at(repository, "/deleteBranchOnMerge"),
    })
}

/// What the viewer may do, from the `viewerCan…` fields the header asks for.
fn capabilities(node: &Value, repository: &Value) -> Capabilities {
    let mut caps = mapping::github_capabilities(mapping::GitHubFacts {
        state: opt_str(node, "/state").unwrap_or(""),
        locked: bool_at(node, "/locked"),
        viewer_did_author: bool_at(node, "/viewerDidAuthor"),
        viewer_can_update: bool_at(node, "/viewerCanUpdate"),
        viewer_can_close: bool_at(node, "/viewerCanClose"),
        viewer_can_reopen: bool_at(node, "/viewerCanReopen"),
        viewer_permission: opt_str(repository, "/viewerPermission"),
    });
    caps.merge = merge_capability(node, repository);
    caps
}

/// An edit handle, only where the forge says the viewer may use it.
fn comment_ref(node: &Value, kind: CommentKind) -> Option<CommentRef> {
    if !bool_at(node, "/viewerCanUpdate") {
        return None;
    }
    opt_str(node, "/id").map(|id| CommentRef {
        id: id.to_string(),
        kind,
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
                edit: comment_ref(node, CommentKind::Comment),
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
                    edit: comment_ref(node, CommentKind::Review),
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
                id: None,
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
        // Only a user's id is what `requestReviews` takes in `userIds`.
        let id = (opt_str(request, "/requestedReviewer/__typename") == Some("User"))
            .then(|| opt_str(request, "/requestedReviewer/id").map(str::to_string))
            .flatten();
        match reviewers
            .iter_mut()
            .find(|reviewer| reviewer.login == login)
        {
            Some(existing) => {
                existing.outcome = ReviewOutcome::Requested;
                existing.id = id;
            }
            None => reviewers.push(Reviewer {
                id,
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
            // Only an Actions job has a workflow run; a third-party check run
            // has none and keeps opening its page.
            job: node
                .pointer("/checkSuite/workflowRun/databaseId")
                .and_then(Value::as_u64)
                .and_then(|run| {
                    Some(CheckJob {
                        job_id: node.get("databaseId").and_then(Value::as_u64)?,
                        run_id: Some(run),
                        retryable: mapping::github_job_retryable(opt_str(node, "/checkSuite/status")),
                    })
                }),
        }),
        "StatusContext" => Some(Check {
            name: opt_str(node, "/context")?.to_string(),
            status: mapping::github_status_context(&str_at(node, "/state")),
            group: None,
            duration_secs: None,
            url: opt_str(node, "/targetUrl").map(str::to_string),
            job: None,
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

/// The pre-flight read of one pull request: its id, its state and what the
/// viewer may do to it now.
fn action_context(client: &ForgeClient, number: u64) -> Result<ActionContext, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "ChangeRequestActionContext",
        ACTION_CONTEXT,
        json!({ "owner": owner, "name": name, "number": number }),
    )?;
    let node = data
        .pointer("/repository/pullRequest")
        .filter(|node| !node.is_null())
        .ok_or_else(|| ForgeError::NotFound {
            host: client.host.clone(),
        })?;
    let node_id = opt_str(node, "/id")
        .ok_or_else(|| ForgeError::UnexpectedResponse {
            host: client.host.clone(),
            detail: "a pull request without an id".to_string(),
        })?
        .to_string();
    let repository = data.pointer("/repository").unwrap_or(&Value::Null);
    let requested = |typename: &str| -> Vec<String> {
        array_at(node, "/reviewRequests/nodes")
            .into_iter()
            .filter(|request| opt_str(request, "/requestedReviewer/__typename") == Some(typename))
            .filter_map(|request| opt_str(request, "/requestedReviewer/id").map(str::to_string))
            .collect()
    };
    Ok(ActionContext {
        node_id,
        state: mapping::github_change_state(&str_at(node, "/state"), bool_at(node, "/isDraft")),
        head_sha: opt_str(node, "/headRefOid").map(str::to_string),
        head_ref_name: opt_str(node, "/headRefName").map(str::to_string),
        cross_repository: bool_at(node, "/isCrossRepository"),
        reviewer_ids: requested("User"),
        team_ids: requested("Team"),
        bot_ids: requested("Bot"),
        unsendable_requests: array_at(node, "/reviewRequests/nodes")
            .into_iter()
            .filter_map(|request| opt_str(request, "/requestedReviewer/__typename"))
            .filter(|typename| !matches!(*typename, "User" | "Team" | "Bot"))
            .map(|typename| match typename {
                "EnterpriseTeam" => "enterprise team".to_string(),
                other => other.to_lowercase(),
            })
            .collect(),
        capabilities: capabilities(node, repository),
    })
}

/// One mutation. Every GitHub mutation takes a single `$input`.
fn mutate(
    client: &ForgeClient,
    operation: &str,
    document: &str,
    input: Value,
) -> Result<(), ForgeError> {
    execute_mutation(client, operation, document, json!({ "input": input })).map(|_| ())
}

pub(crate) fn act(
    client: &ForgeClient,
    number: u64,
    action: &Action,
) -> Result<ActionOutcome, ForgeError> {
    let context = action_context(client, number)?;
    check_action(&client.host, action, &context)?;
    let id = context.node_id.as_str();
    match action {
        Action::Comment { body } => mutate(
            client,
            "AddComment",
            ADD_COMMENT,
            json!({ "subjectId": id, "body": body }),
        )?,
        Action::Review { verdict, body } => {
            let event = match verdict {
                ReviewVerdict::Approve => "APPROVE",
                ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
                ReviewVerdict::Comment => "COMMENT",
            };
            let mut input = json!({ "pullRequestId": id, "event": event });
            if !body.trim().is_empty() {
                input["body"] = json!(body);
            }
            mutate(client, "AddPullRequestReview", ADD_REVIEW, input)?
        }
        Action::Close => mutate(
            client,
            "ClosePullRequest",
            CLOSE,
            json!({ "pullRequestId": id }),
        )?,
        Action::Reopen => mutate(
            client,
            "ReopenPullRequest",
            REOPEN,
            json!({ "pullRequestId": id }),
        )?,
        Action::MarkReady => mutate(
            client,
            "MarkPullRequestReadyForReview",
            READY,
            json!({ "pullRequestId": id }),
        )?,
        Action::ConvertToDraft => mutate(
            client,
            "ConvertPullRequestToDraft",
            DRAFT,
            json!({ "pullRequestId": id }),
        )?,
        Action::Edit {
            title,
            body,
            target_branch,
        } => {
            let mut input = json!({ "pullRequestId": id });
            if let Some(title) = title {
                input["title"] = json!(title);
            }
            if let Some(body) = body {
                input["body"] = json!(body);
            }
            if let Some(branch) = target_branch {
                input["baseRefName"] = json!(branch);
            }
            mutate(client, "UpdatePullRequest", UPDATE, input)?
        }
        Action::Merge {
            method,
            commit_title,
            commit_message,
            delete_branch,
            when_checks_pass,
            expected_head,
        } => {
            let word = match method {
                MergeMethod::Merge => "MERGE",
                MergeMethod::Squash => "SQUASH",
                MergeMethod::Rebase => "REBASE",
            };
            let mut input =
                json!({ "pullRequestId": id, "mergeMethod": word, "expectedHeadOid": expected_head });
            if *method != MergeMethod::Rebase {
                if let Some(title) = commit_title {
                    input["commitHeadline"] = json!(title);
                }
                if let Some(message) = commit_message.as_deref().filter(|m| !m.trim().is_empty()) {
                    input["commitBody"] = json!(message);
                }
            }
            if *when_checks_pass {
                // GitHub's auto-merge has no delete flag: the repository's
                // own setting decides (plan ruling 4).
                mutate(client, "EnablePullRequestAutoMerge", ENABLE_AUTO_MERGE, input)?;
            } else {
                mutate(client, "MergePullRequest", MERGE, input)?;
                if *delete_branch && !context.cross_repository {
                    if let Some(branch) = &context.head_ref_name {
                        return Ok(delete_head(client, branch));
                    }
                }
            }
        }
        Action::CancelAutoMerge => mutate(
            client,
            "DisablePullRequestAutoMerge",
            DISABLE_AUTO_MERGE,
            json!({ "pullRequestId": id }),
        )?,
        Action::SetReviewers { add, remove } if remove.is_empty() => {
            // Adding only: `union: true` leaves every request already there
            // alone, whatever kind of reviewer it names.
            mutate(
                client,
                "RequestReviews",
                REQUEST_REVIEWS,
                json!({ "pullRequestId": id, "userIds": add, "union": true }),
            )?
        }
        Action::SetReviewers { add, remove } => {
            // `union: false` replaces the whole set of requests, so the set
            // is built from the fresh read, never from the tab's copy; a
            // kind it cannot name was refused in `check_action`.
            let mut users: Vec<String> = context
                .reviewer_ids
                .iter()
                .filter(|user| !remove.contains(user))
                .cloned()
                .collect();
            for user in add {
                if !users.contains(user) {
                    users.push(user.clone());
                }
            }
            let teams: Vec<&String> = context.team_ids.iter().filter(|team| !remove.contains(team)).collect();
            let bots: Vec<&String> = context.bot_ids.iter().filter(|bot| !remove.contains(bot)).collect();
            let mut input = json!({ "pullRequestId": id, "userIds": users, "teamIds": teams, "union": false });
            // Named only when there are some: an older GitHub Enterprise
            // without `botIds` would refuse the whole mutation.
            if !bots.is_empty() {
                input["botIds"] = json!(bots);
            }
            mutate(client, "RequestReviews", REQUEST_REVIEWS, input)?
        }
        Action::SetLabels { add, remove } => {
            if !add.is_empty() {
                mutate(
                    client,
                    "AddLabelsToLabelable",
                    ADD_LABELS,
                    json!({ "labelableId": id, "labelIds": add }),
                )?;
            }
            if !remove.is_empty() {
                let removed = mutate(
                    client,
                    "RemoveLabelsFromLabelable",
                    REMOVE_LABELS,
                    json!({ "labelableId": id, "labelIds": remove }),
                );
                match removed {
                    Ok(()) => {}
                    Err(error) if !add.is_empty() => {
                        return Ok(ActionOutcome {
                            warning: Some(format!("labels added, but removing the others failed: {error}")),
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Action::Rerun(target) => {
            let (owner, name) = owner_and_name(client)?;
            let path = match target {
                RerunTarget::FailedInRun(run) => {
                    format!("repos/{owner}/{name}/actions/runs/{run}/rerun-failed-jobs")
                }
                RerunTarget::Job(job) => format!("repos/{owner}/{name}/actions/jobs/{job}/rerun"),
            };
            execute_rest(
                client,
                &RestRequest {
                    method: RestMethod::Post,
                    path,
                    body: None,
                },
            )?;
        }
        Action::EditComment { comment, body } => match comment.kind {
            CommentKind::Comment => mutate(
                client,
                "UpdateIssueComment",
                UPDATE_COMMENT,
                json!({ "id": comment.id, "body": body }),
            )?,
            CommentKind::Review => mutate(
                client,
                "UpdatePullRequestReview",
                UPDATE_REVIEW,
                json!({ "pullRequestReviewId": comment.id, "body": body }),
            )?,
        },
    }
    Ok(ActionOutcome::default())
}

/// After a merge: the head branch goes, through REST — the merge stands
/// whatever this answers (spec §5, *delete branch*).
fn delete_head(client: &ForgeClient, branch: &str) -> ActionOutcome {
    let result = owner_and_name(client).and_then(|(owner, name)| {
        let request = RestRequest {
            method: RestMethod::Delete,
            path: format!("repos/{owner}/{name}/git/refs/heads/{}", percent_encode(branch, true)),
            body: None,
        };
        execute_rest(client, &request).map(|_| ())
    });
    match result {
        Ok(()) => ActionOutcome::default(),
        // The repository deletes merged heads itself and got there first:
        // the branch is gone, which is what was asked.
        Err(ForgeError::Rejected { message, .. }) if message.contains("Reference does not exist") => {
            ActionOutcome::default()
        }
        Err(error) => ActionOutcome {
            warning: Some(format!("merged; deleting the branch failed: {error}")),
        },
    }
}

pub(crate) fn reviewer_candidates(
    client: &ForgeClient,
    number: u64,
    text: &str,
) -> Result<Vec<Candidate>, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "ReviewerCandidates",
        REVIEWER_CANDIDATES,
        json!({ "owner": owner, "name": name, "number": number, "q": text }),
    )?;
    // Nobody reviews their own pull request.
    let author = opt_str(&data, "/repository/pullRequest/author/login").unwrap_or("");
    Ok(array_at(&data, "/repository/assignableUsers/nodes")
        .into_iter()
        .filter_map(|user| {
            let login = opt_str(user, "/login")?;
            (!login.eq_ignore_ascii_case(author)).then(|| Candidate {
                id: opt_str(user, "/id").unwrap_or_default().to_string(),
                label: login.to_string(),
                note: opt_str(user, "/name").filter(|name| !name.is_empty()).map(str::to_string),
            })
        })
        .filter(|candidate| !candidate.id.is_empty())
        .collect())
}

pub(crate) fn label_candidates(client: &ForgeClient, text: &str) -> Result<Vec<Candidate>, ForgeError> {
    let (owner, name) = owner_and_name(client)?;
    let data = run(
        client,
        "LabelCandidates",
        LABEL_CANDIDATES,
        json!({ "owner": owner, "name": name, "q": text }),
    )?;
    Ok(labels(data.pointer("/repository").unwrap_or(&Value::Null))
        .into_iter()
        .map(|label| Candidate {
            id: label.id,
            label: label.name,
            note: None,
        })
        .collect())
}

/// A classic token lists its scopes in `X-OAuth-Scopes` on any answer; the
/// cheapest is `GET user`. A fine-grained token sends no such header.
pub(crate) fn token_scopes(client: &ForgeClient) -> Option<TokenScopes> {
    let request = RestRequest {
        method: RestMethod::Get,
        path: "user".to_string(),
        body: None,
    };
    let response = execute_rest(client, &request).ok()?;
    response.header("x-oauth-scopes").map(TokenScopes::from_header)
}

/// See [`crate::action::live_probes`].
pub(crate) fn live_probes() -> Vec<LiveProbe> {
    let id = "PR_sirio_live_check_0";
    let write = |operation, document, input: Value| LiveProbe {
        operation,
        document,
        variables: json!({ "input": input }),
    };
    vec![
        LiveProbe {
            operation: "ChangeRequestActionContext",
            document: ACTION_CONTEXT,
            variables: json!({ "owner": "ai-sirio", "name": "sirio", "number": 588 }),
        },
        write("AddComment", ADD_COMMENT, json!({ "subjectId": id, "body": "x" })),
        write(
            "AddPullRequestReview",
            ADD_REVIEW,
            json!({ "pullRequestId": id, "event": "COMMENT", "body": "x" }),
        ),
        write("ClosePullRequest", CLOSE, json!({ "pullRequestId": id })),
        write("ReopenPullRequest", REOPEN, json!({ "pullRequestId": id })),
        write("MarkPullRequestReadyForReview", READY, json!({ "pullRequestId": id })),
        write("ConvertPullRequestToDraft", DRAFT, json!({ "pullRequestId": id })),
        write(
            "UpdatePullRequest",
            UPDATE,
            json!({ "pullRequestId": id, "title": "x", "body": "x", "baseRefName": "x" }),
        ),
        write(
            "UpdateIssueComment",
            UPDATE_COMMENT,
            json!({ "id": "IC_sirio_live_check_0", "body": "x" }),
        ),
        write(
            "UpdatePullRequestReview",
            UPDATE_REVIEW,
            json!({ "pullRequestReviewId": "PRR_sirio_live_check_0", "body": "x" }),
        ),
        write(
            "MergePullRequest",
            MERGE,
            json!({ "pullRequestId": id, "mergeMethod": "SQUASH", "commitHeadline": "x", "commitBody": "x",
                    "expectedHeadOid": "0000000000000000000000000000000000000000" }),
        ),
        write(
            "EnablePullRequestAutoMerge",
            ENABLE_AUTO_MERGE,
            json!({ "pullRequestId": id, "mergeMethod": "MERGE",
                    "expectedHeadOid": "0000000000000000000000000000000000000000" }),
        ),
        write("DisablePullRequestAutoMerge", DISABLE_AUTO_MERGE, json!({ "pullRequestId": id })),
        write(
            "RequestReviews",
            REQUEST_REVIEWS,
            json!({ "pullRequestId": id, "userIds": ["U_sirio_live_check_0"], "teamIds": [], "union": false }),
        ),
        write(
            "AddLabelsToLabelable",
            ADD_LABELS,
            json!({ "labelableId": id, "labelIds": ["LA_sirio_live_check_0"] }),
        ),
        write(
            "RemoveLabelsFromLabelable",
            REMOVE_LABELS,
            json!({ "labelableId": id, "labelIds": ["LA_sirio_live_check_0"] }),
        ),
        LiveProbe {
            operation: "ReviewerCandidates",
            document: REVIEWER_CANDIDATES,
            variables: json!({ "owner": "ai-sirio", "name": "sirio", "number": 588, "q": "e" }),
        },
        LiveProbe {
            operation: "LabelCandidates",
            document: LABEL_CANDIDATES,
            variables: json!({ "owner": "ai-sirio", "name": "sirio", "q": "bug" }),
        },
    ]
}

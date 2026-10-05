//! Pure translations from each forge's vocabulary into the model's. Every
//! unknown value lands on a neutral member: a forge adding a state must
//! never make a list fail to draw.

use crate::model::{
    BlockReason, Capabilities, MergeCapability, MergeMethod, MergeMethods, MergeVerdict, ChangeState, CheckStatus, CiState, EventKind, FileChangeKind, Progress,
    ReviewOutcome, ReviewState, Side,
};

pub(crate) fn github_change_state(state: &str, is_draft: bool) -> ChangeState {
    match state {
        "MERGED" => ChangeState::Merged,
        "CLOSED" => ChangeState::Closed,
        _ if is_draft => ChangeState::Draft,
        _ => ChangeState::Open,
    }
}

pub(crate) fn gitlab_change_state(state: &str, draft: bool) -> ChangeState {
    match state {
        "merged" => ChangeState::Merged,
        "closed" | "locked" => ChangeState::Closed,
        _ if draft => ChangeState::Draft,
        _ => ChangeState::Open,
    }
}

/// GitLab moves a discussion's position forward on every push it can; one
/// it could not move still names an older head (verified on gitlab.com,
/// 2026-10-05). Without both shas nothing is known, and the thread is drawn
/// as current.
pub(crate) fn gitlab_thread_outdated(position_head: Option<&str>, current_head: Option<&str>) -> bool {
    matches!((position_head, current_head), (Some(position), Some(current)) if position != current)
}

/// A position names a new line, an old line, or both (a context line, read
/// on the new side).
pub(crate) fn gitlab_anchor(new_line: Option<u32>, old_line: Option<u32>) -> (Side, Option<u32>) {
    match (new_line, old_line) {
        (Some(line), _) => (Side::New, Some(line)),
        (None, Some(line)) => (Side::Old, Some(line)),
        (None, None) => (Side::New, None),
    }
}

/// One `{ state, count }` pair of a GitHub `…CountsByState` list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateCount<'a> {
    pub state: &'a str,
    pub count: u32,
}

/// A failing rollup shows as failed even while other checks run: it is the
/// fact the user must act on. Otherwise a pending rollup reports how many
/// checks and commit statuses have finished.
pub(crate) fn github_ci(
    rollup: Option<&str>,
    check_runs: &[StateCount<'_>],
    status_contexts: &[StateCount<'_>],
) -> CiState {
    let Some(rollup) = rollup else {
        return CiState::NoChecks;
    };
    let total: u32 = check_runs
        .iter()
        .chain(status_contexts)
        .map(|pair| pair.count)
        .sum();
    let unfinished: u32 = check_runs
        .iter()
        .filter(|pair| {
            matches!(
                pair.state,
                "IN_PROGRESS" | "PENDING" | "QUEUED" | "WAITING" | "REQUESTED"
            )
        })
        .chain(
            status_contexts
                .iter()
                .filter(|pair| matches!(pair.state, "PENDING" | "EXPECTED")),
        )
        .map(|pair| pair.count)
        .sum();
    match rollup {
        "SUCCESS" => CiState::Passed,
        "FAILURE" | "ERROR" => CiState::Failed,
        "PENDING" | "EXPECTED" => CiState::Running((total > 0).then(|| Progress {
            done: total.saturating_sub(unfinished),
            total,
        })),
        _ => CiState::NoChecks,
    }
}

/// A pipeline waiting on a manual job has not finished, so it reads as
/// running. The counts are absent on the baseline queries.
pub(crate) fn gitlab_ci(
    status: Option<&str>,
    total_jobs: Option<u32>,
    finished_jobs: Option<u32>,
) -> CiState {
    let progress = match (finished_jobs, total_jobs) {
        (Some(done), Some(total)) if total > 0 => Some(Progress {
            done: done.min(total),
            total,
        }),
        _ => None,
    };
    match status {
        Some("SUCCESS") => CiState::Passed,
        Some("FAILED") => CiState::Failed,
        Some("CANCELED" | "CANCELING") => CiState::Canceled,
        Some(
            "CREATED"
            | "WAITING_FOR_RESOURCE"
            | "PREPARING"
            | "WAITING_FOR_CALLBACK"
            | "PENDING"
            | "RUNNING"
            | "SCHEDULED"
            | "MANUAL",
        ) => CiState::Running(progress),
        _ => CiState::NoChecks,
    }
}

pub(crate) fn github_check_run(status: &str, conclusion: Option<&str>) -> CheckStatus {
    match status {
        "COMPLETED" => match conclusion {
            Some("SUCCESS") => CheckStatus::Passed,
            Some("FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED") => {
                CheckStatus::Failed
            }
            Some("CANCELLED") => CheckStatus::Canceled,
            Some("SKIPPED") => CheckStatus::Skipped,
            _ => CheckStatus::Neutral,
        },
        "IN_PROGRESS" => CheckStatus::Running,
        "QUEUED" | "PENDING" | "WAITING" | "REQUESTED" => CheckStatus::Queued,
        _ => CheckStatus::Neutral,
    }
}

pub(crate) fn github_status_context(state: &str) -> CheckStatus {
    match state {
        "SUCCESS" => CheckStatus::Passed,
        "FAILURE" | "ERROR" => CheckStatus::Failed,
        "PENDING" | "EXPECTED" => CheckStatus::Running,
        _ => CheckStatus::Neutral,
    }
}

/// A manual job is waiting for a person, not failing, so it is neutral.
pub(crate) fn gitlab_job(status: &str) -> CheckStatus {
    match status {
        "SUCCESS" => CheckStatus::Passed,
        "FAILED" => CheckStatus::Failed,
        "CANCELED" | "CANCELING" => CheckStatus::Canceled,
        "SKIPPED" => CheckStatus::Skipped,
        "RUNNING" => CheckStatus::Running,
        "CREATED"
        | "WAITING_FOR_RESOURCE"
        | "PREPARING"
        | "WAITING_FOR_CALLBACK"
        | "PENDING"
        | "SCHEDULED" => CheckStatus::Queued,
        _ => CheckStatus::Neutral,
    }
}

/// A GitLab job (REST status, lower case) whose log will not grow. Unknown
/// is not settled: the tab keeps reloading rather than freeze a live log.
pub(crate) fn gitlab_job_settled(status: &str) -> bool {
    matches!(status, "success" | "failed" | "canceled" | "skipped")
}

/// The number at the end of a GitLab global id — `gid://gitlab/Ci::Build/42`
/// is 42 — or `None` for anything else.
pub(crate) fn gitlab_gid_number(gid: &str) -> Option<u64> {
    let (prefix, number) = gid.rsplit_once('/')?;
    if !prefix.starts_with("gid://") || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

/// GitHub lets whoever can write to the repository re-run its workflows.
/// An unknown permission is read as no permission.
pub(crate) fn github_can_rerun(permission: Option<&str>) -> bool {
    matches!(permission, Some("ADMIN" | "MAINTAIN" | "WRITE"))
}

/// GitHub re-runs a job only once the workflow run it belongs to has
/// finished; `suite_status` is its check suite's `status`.
pub(crate) fn github_job_retryable(suite_status: Option<&str>) -> bool {
    suite_status == Some("COMPLETED")
}

/// GitHub's own decision wins; without one (a repository with no review
/// policy) approvals still show.
pub(crate) fn github_review(decision: Option<&str>, approvals: u32) -> ReviewState {
    match decision {
        Some("APPROVED") => ReviewState::Approved {
            count: approvals.max(1),
        },
        Some("CHANGES_REQUESTED") => ReviewState::ChangesRequested,
        Some("REVIEW_REQUIRED") => ReviewState::ReviewRequired,
        _ if approvals > 0 => ReviewState::Approved { count: approvals },
        _ => ReviewState::None,
    }
}

/// `None` for `PENDING`: an unsubmitted review is the viewer's own draft.
pub(crate) fn github_review_outcome(state: &str) -> Option<ReviewOutcome> {
    match state {
        "PENDING" => None,
        "APPROVED" => Some(ReviewOutcome::Approved),
        "CHANGES_REQUESTED" => Some(ReviewOutcome::ChangesRequested),
        "COMMENTED" => Some(ReviewOutcome::Commented),
        "DISMISSED" => Some(ReviewOutcome::Dismissed),
        _ => Some(ReviewOutcome::Other),
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GitLabReviewer<'a> {
    pub username: &'a str,
    pub review_state: Option<&'a str>,
}

/// Any request for changes outranks approvals. On the baseline queries the
/// reviewers carry no state, so a reviewer list alone means "required".
pub(crate) fn gitlab_review(reviewers: &[GitLabReviewer<'_>], approvals: u32) -> ReviewState {
    if reviewers
        .iter()
        .any(|reviewer| reviewer.review_state == Some("REQUESTED_CHANGES"))
    {
        ReviewState::ChangesRequested
    } else if approvals > 0 {
        ReviewState::Approved { count: approvals }
    } else if !reviewers.is_empty() {
        ReviewState::ReviewRequired
    } else {
        ReviewState::None
    }
}

/// On the baseline a reviewer entry carries no state; being listed is then
/// the only evidence, and it counts as pending.
pub(crate) fn gitlab_review_requested_from(reviewers: &[GitLabReviewer<'_>], me: &str) -> bool {
    reviewers.iter().any(|reviewer| {
        reviewer.username.eq_ignore_ascii_case(me)
            && matches!(
                reviewer.review_state,
                None | Some("UNREVIEWED" | "REVIEW_STARTED" | "UNAPPROVED")
            )
    })
}

pub(crate) fn gitlab_reviewer_outcome(state: Option<&str>) -> ReviewOutcome {
    match state {
        Some("APPROVED") => ReviewOutcome::Approved,
        Some("REQUESTED_CHANGES") => ReviewOutcome::ChangesRequested,
        Some("REVIEWED") => ReviewOutcome::Commented,
        None | Some("UNREVIEWED" | "REVIEW_STARTED" | "UNAPPROVED") => ReviewOutcome::Requested,
        Some(_) => ReviewOutcome::Other,
    }
}

/// What a GitLab system note means for the timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SystemNote {
    Approved,
    Event(EventKind),
}

/// GitLab writes system notes in prose; these are the ones the timeline
/// gives a meaning to. Anything else is kept as its first line.
pub(crate) fn gitlab_system_note(body: &str) -> SystemNote {
    let first = body.lines().next().unwrap_or("").trim();
    if first == "approved this merge request" {
        return SystemNote::Approved;
    }
    if let Some(rest) = first.strip_prefix("added ")
        && rest.contains("commit")
    {
        let count = rest
            .split_whitespace()
            .next()
            .and_then(|word| word.parse().ok())
            .unwrap_or(1);
        return SystemNote::Event(EventKind::CommitsPushed { count });
    }
    if let Some(rest) = first.strip_prefix("requested review from ") {
        let reviewer = rest
            .split([',', ' '])
            .next()
            .unwrap_or("")
            .trim_start_matches('@')
            .to_string();
        return SystemNote::Event(EventKind::ReviewRequested { reviewer });
    }
    let kind = match first {
        "merged" => EventKind::Merged,
        "closed" => EventKind::Closed,
        "reopened" => EventKind::Reopened,
        _ if first.starts_with("marked this merge request as **ready**") => {
            EventKind::ReadyForReview
        }
        _ if first.starts_with("marked this merge request as **draft**") => {
            EventKind::ConvertedToDraft
        }
        _ => EventKind::Other(first.to_string()),
    };
    SystemNote::Event(kind)
}

/// The facts `github_capabilities` reads, so the mapping is one pure step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitHubFacts<'a> {
    pub state: &'a str,
    pub locked: bool,
    pub viewer_did_author: bool,
    pub viewer_can_update: bool,
    pub viewer_can_close: bool,
    pub viewer_can_reopen: bool,
    pub viewer_permission: Option<&'a str>,
}

/// GitHub says what the viewer may do in `viewerCan…` fields; what a state
/// makes meaningless is Sirio's to remove. Nobody reviews their own pull
/// request, and a finished one takes no review, close or draft toggle.
pub(crate) fn github_capabilities(facts: GitHubFacts<'_>) -> Capabilities {
    let open = facts.state == "OPEN";
    let reviewable = open && !facts.viewer_did_author;
    Capabilities {
        can_comment: !facts.locked,
        can_approve: reviewable,
        can_request_changes: reviewable,
        can_edit: facts.viewer_can_update,
        can_change_state: match facts.state {
            "OPEN" => facts.viewer_can_close,
            "CLOSED" => facts.viewer_can_reopen,
            _ => false,
        },
        can_toggle_draft: open && facts.viewer_can_update,
        can_edit_reviewers: facts.viewer_can_update,
        can_edit_labels: facts.viewer_can_update,
        can_rerun_checks: github_can_rerun(facts.viewer_permission),
        merge: MergeCapability::default(),
    }
}

/// The facts `gitlab_capabilities` reads. `None` is "the server did not
/// report it" — a baseline query never asks, and an old server never knew.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitLabFacts<'a> {
    pub state: &'a str,
    pub locked: bool,
    pub can_create_note: Option<bool>,
    pub can_update: Option<bool>,
    pub can_approve: Option<bool>,
    /// Reviewers carry a review state on this server — the same generation
    /// of GitLab that can request changes.
    pub reports_review_state: bool,
    pub can_update_pipeline: Option<bool>,
}

pub(crate) fn gitlab_capabilities(facts: GitLabFacts<'_>) -> Capabilities {
    let opened = facts.state == "opened";
    let can_update = facts.can_update == Some(true);
    let can_approve = opened && facts.can_approve == Some(true);
    Capabilities {
        can_comment: !facts.locked && facts.can_create_note != Some(false),
        can_approve,
        can_request_changes: can_approve && facts.reports_review_state,
        can_edit: can_update,
        can_change_state: can_update && matches!(facts.state, "opened" | "closed"),
        can_toggle_draft: can_update && opened,
        can_edit_reviewers: can_update,
        can_edit_labels: can_update,
        can_rerun_checks: facts.can_update_pipeline == Some(true),
        merge: MergeCapability::default(),
    }
}

/// What `github_merge` reads: the pull request's merge state and the
/// repository's merge settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitHubMergeFacts<'a> {
    /// Sirio's own state word: `OPEN`, `DRAFT`, `CLOSED` or `MERGED`.
    pub state: &'a str,
    pub merge_state_status: Option<&'a str>,
    pub review_decision: Option<&'a str>,
    /// The head commit's `statusCheckRollup.state`.
    pub rollup: Option<&'a str>,
    pub merge_commit_allowed: bool,
    pub squash_merge_allowed: bool,
    pub rebase_merge_allowed: bool,
    pub auto_merge_allowed: bool,
    pub viewer_can_enable_auto_merge: bool,
    /// `autoMergeRequest.mergeMethod`, when an auto-merge is set.
    pub auto_merge_method: Option<&'a str>,
    pub delete_branch_on_merge: bool,
}

/// GitHub's `BLOCKED` covers both a missing review and running required
/// checks; the review decision and the rollup tell them apart. A status
/// GitHub adds later is a block named in GitHub's own word.
pub(crate) fn github_merge(facts: GitHubMergeFacts<'_>) -> MergeCapability {
    let methods = MergeMethods {
        merge: facts.merge_commit_allowed,
        squash: facts.squash_merge_allowed,
        rebase: facts.rebase_merge_allowed,
    };
    let verdict = match facts.merge_state_status {
        None => MergeVerdict::Unreported,
        Some(_) if methods.is_empty() => MergeVerdict::Blocked(BlockReason::NoMethod),
        Some(_) if facts.state == "DRAFT" => MergeVerdict::Blocked(BlockReason::Draft),
        Some("CLEAN" | "UNSTABLE" | "HAS_HOOKS") => MergeVerdict::Ready,
        Some("DIRTY") => MergeVerdict::Blocked(BlockReason::Conflicts),
        Some("BEHIND") => MergeVerdict::Blocked(BlockReason::Behind),
        Some("BLOCKED") => match (facts.review_decision, facts.rollup) {
            (Some("CHANGES_REQUESTED"), _) => MergeVerdict::Blocked(BlockReason::ChangesRequested),
            (Some("REVIEW_REQUIRED"), _) => MergeVerdict::Blocked(BlockReason::ReviewRequired),
            (_, Some("PENDING" | "EXPECTED")) => MergeVerdict::WaitingOnChecks,
            (_, Some("FAILURE" | "ERROR")) => MergeVerdict::Blocked(BlockReason::ChecksFailing),
            _ => MergeVerdict::Blocked(BlockReason::Other("BLOCKED".to_string())),
        },
        // GitHub works mergeability out lazily, after a push to either side.
        Some("UNKNOWN") => MergeVerdict::Checking,
        Some(word) => MergeVerdict::Blocked(BlockReason::Other(word.to_string())),
    };
    MergeCapability {
        verdict,
        methods,
        default_method: methods.list().first().copied(),
        can_auto_merge: facts.auto_merge_allowed && facts.viewer_can_enable_auto_merge,
        auto_merge_enabled: facts.auto_merge_method.map(|word| match word {
            "SQUASH" => MergeMethod::Squash,
            "REBASE" => MergeMethod::Rebase,
            _ => MergeMethod::Merge,
        }),
        delete_branch_default: facts.delete_branch_on_merge,
    }
}

/// What `gitlab_merge` reads. `None` is "the server did not report it".
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitLabMergeFacts<'a> {
    pub state: &'a str,
    pub detailed_status: Option<&'a str>,
    pub can_merge: Option<bool>,
    pub squash_read_only: Option<bool>,
    pub squash_on_merge: Option<bool>,
    pub auto_merge_enabled: Option<bool>,
    pub auto_merge_strategies: Option<&'a [&'a str]>,
    pub remove_source_branch: Option<bool>,
}

/// GitLab picks the merge method per project, not per merge: Sirio offers
/// a merge commit and, where the project lets the user choose, a squash.
pub(crate) fn gitlab_merge(facts: GitLabMergeFacts<'_>) -> MergeCapability {
    let (Some(status), Some(can_merge)) = (facts.detailed_status, facts.can_merge) else {
        return MergeCapability::default();
    };
    let read_only = facts.squash_read_only == Some(true);
    let squash_on = facts.squash_on_merge == Some(true);
    let methods = MergeMethods {
        merge: !(read_only && squash_on),
        squash: !read_only || squash_on,
        rebase: false,
    };
    let verdict = if !can_merge {
        MergeVerdict::Blocked(BlockReason::Other("cannot merge".to_string()))
    } else if facts.state != "opened" {
        MergeVerdict::Blocked(BlockReason::Other("not open".to_string()))
    } else {
        match status {
            "MERGEABLE" => MergeVerdict::Ready,
            "CI_STILL_RUNNING" => MergeVerdict::WaitingOnChecks,
            "CI_MUST_PASS" => MergeVerdict::Blocked(BlockReason::ChecksFailing),
            "NOT_APPROVED" => MergeVerdict::Blocked(BlockReason::ReviewRequired),
            "REQUESTED_CHANGES" => MergeVerdict::Blocked(BlockReason::ChangesRequested),
            "CONFLICT" => MergeVerdict::Blocked(BlockReason::Conflicts),
            "NEED_REBASE" => MergeVerdict::Blocked(BlockReason::Behind),
            "DRAFT_STATUS" => MergeVerdict::Blocked(BlockReason::Draft),
            "DISCUSSIONS_NOT_RESOLVED" => MergeVerdict::Blocked(BlockReason::Discussions),
            // Passing states: GitLab has not finished its checks yet.
            "UNCHECKED" | "CHECKING" | "PREPARING" | "APPROVALS_SYNCING" => MergeVerdict::Checking,
            word => MergeVerdict::Blocked(BlockReason::Other(word.to_string())),
        }
    };
    let default_method = if squash_on && methods.squash {
        Some(MergeMethod::Squash)
    } else {
        methods.list().first().copied()
    };
    MergeCapability {
        verdict,
        methods,
        default_method,
        // GitLab lists its strategies as its own lower-case constants
        // (`merge_when_checks_pass`); the mutation's enum spells them in
        // capitals. Either spelling is read.
        can_auto_merge: facts.auto_merge_strategies.is_some_and(|strategies| {
            strategies.iter().any(|strategy| strategy.eq_ignore_ascii_case("merge_when_checks_pass"))
        }),
        auto_merge_enabled: (facts.auto_merge_enabled == Some(true)).then(|| {
            if squash_on { MergeMethod::Squash } else { MergeMethod::Merge }
        }),
        delete_branch_default: facts.remove_source_branch == Some(true),
    }
}

pub(crate) fn unix_seconds(timestamp: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|time| time.timestamp())
}

pub(crate) fn file_change_kind(change_type: &str) -> Option<FileChangeKind> {
    match change_type {
        "ADDED" => Some(FileChangeKind::Added),
        "MODIFIED" | "CHANGED" => Some(FileChangeKind::Modified),
        "DELETED" => Some(FileChangeKind::Deleted),
        "RENAMED" => Some(FileChangeKind::Renamed),
        "COPIED" => Some(FileChangeKind::Copied),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gitlab_thread_is_outdated_only_when_its_head_is_known_and_not_the_current_one() {
        assert!(!gitlab_thread_outdated(Some("abc"), Some("abc")));
        assert!(gitlab_thread_outdated(Some("old"), Some("abc")));
        // Without either sha nothing can be said: drawn as current.
        assert!(!gitlab_thread_outdated(None, Some("abc")));
        assert!(!gitlab_thread_outdated(Some("abc"), None));
    }

    #[test]
    fn a_gitlab_position_anchors_on_its_new_line_else_its_old_one() {
        assert_eq!(gitlab_anchor(Some(12), None), (Side::New, Some(12)));
        // A context line has both: it is read on the new side.
        assert_eq!(gitlab_anchor(Some(12), Some(10)), (Side::New, Some(12)));
        assert_eq!(gitlab_anchor(None, Some(7)), (Side::Old, Some(7)));
        assert_eq!(gitlab_anchor(None, None), (Side::New, None));
    }

    fn counts<'a>(pairs: &[(&'a str, u32)]) -> Vec<StateCount<'a>> {
        pairs
            .iter()
            .map(|&(state, count)| StateCount { state, count })
            .collect()
    }

    fn reviewers<'a>(pairs: &[(&'a str, Option<&'a str>)]) -> Vec<GitLabReviewer<'a>> {
        pairs
            .iter()
            .map(|&(username, review_state)| GitLabReviewer {
                username,
                review_state,
            })
            .collect()
    }

    #[test]
    fn a_merged_draft_reads_as_merged_on_both_forges() {
        assert_eq!(github_change_state("MERGED", true), ChangeState::Merged);
        assert_eq!(gitlab_change_state("merged", true), ChangeState::Merged);
    }

    #[test]
    fn an_open_draft_reads_as_draft() {
        assert_eq!(github_change_state("OPEN", true), ChangeState::Draft);
        assert_eq!(gitlab_change_state("opened", true), ChangeState::Draft);
    }

    #[test]
    fn a_locked_merge_request_reads_as_closed() {
        assert_eq!(gitlab_change_state("locked", false), ChangeState::Closed);
        assert_eq!(github_change_state("CLOSED", false), ChangeState::Closed);
    }

    #[test]
    fn an_unknown_state_reads_as_open_never_as_finished() {
        assert_eq!(github_change_state("ARCHIVED", false), ChangeState::Open);
        assert_eq!(gitlab_change_state("frozen", false), ChangeState::Open);
    }

    #[test]
    fn a_github_head_without_a_rollup_has_no_checks() {
        assert_eq!(github_ci(None, &[], &[]), CiState::NoChecks);
    }

    #[test]
    fn a_pending_github_rollup_counts_what_finished() {
        let runs = counts(&[("SUCCESS", 4), ("IN_PROGRESS", 2), ("QUEUED", 1)]);
        assert_eq!(
            github_ci(Some("PENDING"), &runs, &[]),
            CiState::Running(Some(Progress { done: 4, total: 7 }))
        );
    }

    #[test]
    fn commit_statuses_count_toward_the_total() {
        let statuses = counts(&[("SUCCESS", 1), ("PENDING", 1)]);
        assert_eq!(
            github_ci(Some("PENDING"), &[], &statuses),
            CiState::Running(Some(Progress { done: 1, total: 2 }))
        );
    }

    #[test]
    fn a_github_failure_shows_while_other_checks_still_run() {
        let runs = counts(&[("FAILURE", 1), ("IN_PROGRESS", 3)]);
        assert_eq!(github_ci(Some("FAILURE"), &runs, &[]), CiState::Failed);
        assert_eq!(github_ci(Some("ERROR"), &[], &[]), CiState::Failed);
    }

    #[test]
    fn an_unknown_github_rollup_is_neutral() {
        assert_eq!(
            github_ci(Some("SOMETHING_NEW"), &[], &[]),
            CiState::NoChecks
        );
    }

    #[test]
    fn a_blocked_gitlab_pipeline_is_still_running() {
        assert_eq!(
            gitlab_ci(Some("MANUAL"), Some(7), Some(3)),
            CiState::Running(Some(Progress { done: 3, total: 7 }))
        );
    }

    #[test]
    fn a_gitlab_pipeline_on_the_baseline_runs_without_counts() {
        assert_eq!(
            gitlab_ci(Some("RUNNING"), None, None),
            CiState::Running(None)
        );
    }

    #[test]
    fn finished_jobs_never_exceed_the_total() {
        assert_eq!(
            gitlab_ci(Some("RUNNING"), Some(3), Some(5)),
            CiState::Running(Some(Progress { done: 3, total: 3 }))
        );
    }

    #[test]
    fn gitlab_verdicts_map_to_their_meaning() {
        assert_eq!(
            gitlab_ci(Some("SUCCESS"), Some(5), Some(5)),
            CiState::Passed
        );
        assert_eq!(gitlab_ci(Some("FAILED"), None, None), CiState::Failed);
        assert_eq!(gitlab_ci(Some("CANCELED"), None, None), CiState::Canceled);
        assert_eq!(gitlab_ci(Some("SKIPPED"), None, None), CiState::NoChecks);
        assert_eq!(gitlab_ci(None, None, None), CiState::NoChecks);
        assert_eq!(
            gitlab_ci(Some("SOMETHING_NEW"), None, None),
            CiState::NoChecks
        );
    }

    #[test]
    fn a_completed_check_run_reads_its_conclusion() {
        assert_eq!(
            github_check_run("COMPLETED", Some("SUCCESS")),
            CheckStatus::Passed
        );
        assert_eq!(
            github_check_run("COMPLETED", Some("ACTION_REQUIRED")),
            CheckStatus::Failed
        );
        assert_eq!(
            github_check_run("COMPLETED", Some("TIMED_OUT")),
            CheckStatus::Failed
        );
        assert_eq!(
            github_check_run("COMPLETED", Some("CANCELLED")),
            CheckStatus::Canceled
        );
        assert_eq!(
            github_check_run("COMPLETED", Some("SKIPPED")),
            CheckStatus::Skipped
        );
        assert_eq!(
            github_check_run("COMPLETED", Some("SOMETHING_NEW")),
            CheckStatus::Neutral
        );
        assert_eq!(github_check_run("COMPLETED", None), CheckStatus::Neutral);
    }

    #[test]
    fn an_unfinished_check_run_is_queued_or_running() {
        assert_eq!(github_check_run("IN_PROGRESS", None), CheckStatus::Running);
        assert_eq!(github_check_run("WAITING", None), CheckStatus::Queued);
        assert_eq!(
            github_check_run("SOMETHING_NEW", None),
            CheckStatus::Neutral
        );
    }

    #[test]
    fn a_pending_commit_status_is_running() {
        assert_eq!(github_status_context("PENDING"), CheckStatus::Running);
        assert_eq!(github_status_context("ERROR"), CheckStatus::Failed);
        assert_eq!(github_status_context("SOMETHING_NEW"), CheckStatus::Neutral);
    }

    #[test]
    fn a_manual_gitlab_job_is_neutral_not_failed() {
        assert_eq!(gitlab_job("MANUAL"), CheckStatus::Neutral);
        assert_eq!(gitlab_job("CREATED"), CheckStatus::Queued);
        assert_eq!(gitlab_job("FAILED"), CheckStatus::Failed);
        assert_eq!(gitlab_job("SOMETHING_NEW"), CheckStatus::Neutral);
    }

    #[test]
    fn github_review_decisions_win_over_counts() {
        assert_eq!(
            github_review(Some("CHANGES_REQUESTED"), 2),
            ReviewState::ChangesRequested
        );
        assert_eq!(
            github_review(Some("REVIEW_REQUIRED"), 1),
            ReviewState::ReviewRequired
        );
        assert_eq!(
            github_review(Some("APPROVED"), 0),
            ReviewState::Approved { count: 1 }
        );
    }

    #[test]
    fn without_a_decision_github_approvals_still_show() {
        assert_eq!(github_review(None, 2), ReviewState::Approved { count: 2 });
        assert_eq!(github_review(None, 0), ReviewState::None);
        assert_eq!(github_review(Some("SOMETHING_NEW"), 0), ReviewState::None);
    }

    #[test]
    fn a_pending_github_review_is_the_viewers_own_draft_and_is_hidden() {
        assert_eq!(github_review_outcome("PENDING"), None);
        assert_eq!(
            github_review_outcome("APPROVED"),
            Some(ReviewOutcome::Approved)
        );
        assert_eq!(
            github_review_outcome("SOMETHING_NEW"),
            Some(ReviewOutcome::Other)
        );
    }

    #[test]
    fn a_gitlab_change_request_beats_its_approvals() {
        let people = reviewers(&[
            ("carol", Some("REQUESTED_CHANGES")),
            ("dave", Some("APPROVED")),
        ]);
        assert_eq!(gitlab_review(&people, 1), ReviewState::ChangesRequested);
    }

    #[test]
    fn gitlab_approvals_and_waiting_reviewers() {
        assert_eq!(gitlab_review(&[], 2), ReviewState::Approved { count: 2 });
        let waiting = reviewers(&[("carol", Some("UNREVIEWED"))]);
        assert_eq!(gitlab_review(&waiting, 0), ReviewState::ReviewRequired);
        let baseline = reviewers(&[("carol", None)]);
        assert_eq!(gitlab_review(&baseline, 0), ReviewState::ReviewRequired);
        assert_eq!(gitlab_review(&[], 0), ReviewState::None);
    }

    #[test]
    fn a_gitlab_review_is_pending_from_me_until_i_answer() {
        assert!(gitlab_review_requested_from(
            &reviewers(&[("Fake-User", Some("UNREVIEWED"))]),
            "fake-user"
        ));
        assert!(gitlab_review_requested_from(
            &reviewers(&[("fake-user", None)]),
            "fake-user"
        ));
        assert!(!gitlab_review_requested_from(
            &reviewers(&[("fake-user", Some("REVIEWED"))]),
            "fake-user"
        ));
        assert!(!gitlab_review_requested_from(
            &reviewers(&[("carol", Some("UNREVIEWED"))]),
            "fake-user"
        ));
    }

    #[test]
    fn gitlab_system_notes_become_timeline_events() {
        assert_eq!(
            gitlab_system_note("approved this merge request"),
            SystemNote::Approved
        );
        assert_eq!(
            gitlab_system_note("added 3 commits\n\n<ul><li>abc - first</li></ul>"),
            SystemNote::Event(EventKind::CommitsPushed { count: 3 })
        );
        assert_eq!(
            gitlab_system_note("added 1 commit\n\n<ul><li>abc</li></ul>"),
            SystemNote::Event(EventKind::CommitsPushed { count: 1 })
        );
        assert_eq!(
            gitlab_system_note("requested review from @fake-user"),
            SystemNote::Event(EventKind::ReviewRequested {
                reviewer: "fake-user".into()
            })
        );
        assert_eq!(
            gitlab_system_note("merged"),
            SystemNote::Event(EventKind::Merged)
        );
        assert_eq!(
            gitlab_system_note("closed"),
            SystemNote::Event(EventKind::Closed)
        );
        assert_eq!(
            gitlab_system_note("reopened"),
            SystemNote::Event(EventKind::Reopened)
        );
        assert_eq!(
            gitlab_system_note("marked this merge request as **ready**"),
            SystemNote::Event(EventKind::ReadyForReview)
        );
        assert_eq!(
            gitlab_system_note("marked this merge request as **draft**"),
            SystemNote::Event(EventKind::ConvertedToDraft)
        );
    }

    #[test]
    fn an_unknown_system_note_keeps_only_its_first_line() {
        assert_eq!(
            gitlab_system_note("mentioned in issue #3\n\nmore"),
            SystemNote::Event(EventKind::Other("mentioned in issue #3".into()))
        );
    }

    #[test]
    fn both_forges_timestamp_shapes_parse() {
        assert_eq!(unix_seconds("2026-09-25T07:58:52Z"), Some(1_790_323_132));
        assert_eq!(
            unix_seconds("2026-09-03T14:43:43+01:00"),
            Some(1_788_443_023)
        );
        assert_eq!(unix_seconds("yesterday"), None);
    }

    #[test]
    fn github_change_types_map_and_unknown_ones_are_none() {
        assert_eq!(file_change_kind("RENAMED"), Some(FileChangeKind::Renamed));
        assert_eq!(file_change_kind("CHANGED"), Some(FileChangeKind::Modified));
        assert_eq!(file_change_kind("SOMETHING_NEW"), None);
    }

    #[test]
    fn a_gitlab_job_is_settled_only_in_a_final_state() {
        for status in ["success", "failed", "canceled", "skipped"] {
            assert!(gitlab_job_settled(status), "{status}");
        }
        for status in ["created", "pending", "running", "manual", "scheduled", "waiting_for_resource", "preparing", "canceling", "something_new"] {
            assert!(!gitlab_job_settled(status), "{status}");
        }
    }

    #[test]
    fn a_gitlab_global_id_gives_its_number_or_nothing() {
        assert_eq!(gitlab_gid_number("gid://gitlab/Ci::Build/42"), Some(42));
        assert_eq!(gitlab_gid_number("gid://gitlab/Ci::Pipeline/7"), Some(7));
        assert_eq!(gitlab_gid_number("gid://gitlab/Ci::Build/"), None);
        assert_eq!(gitlab_gid_number("gid://gitlab/Ci::Build/-1"), None);
        assert_eq!(gitlab_gid_number("gid://gitlab/Ci::Build/4x"), None);
        assert_eq!(gitlab_gid_number("42"), None);
        assert_eq!(gitlab_gid_number(""), None);
    }

    #[test]
    fn only_write_access_or_above_may_rerun_on_github() {
        for permission in ["ADMIN", "MAINTAIN", "WRITE"] {
            assert!(github_can_rerun(Some(permission)), "{permission}");
        }
        for permission in ["TRIAGE", "READ", "write", "SOMETHING_NEW"] {
            assert!(!github_can_rerun(Some(permission)), "{permission}");
        }
        assert!(!github_can_rerun(None));
    }

    #[test]
    fn a_github_job_is_retryable_only_once_its_run_has_finished() {
        assert!(github_job_retryable(Some("COMPLETED")));
        for status in ["IN_PROGRESS", "QUEUED", "WAITING", "PENDING", "REQUESTED"] {
            assert!(!github_job_retryable(Some(status)), "{status}");
        }
        assert!(!github_job_retryable(None));
    }

    #[test]
    fn rerunning_follows_the_forges_permission_and_nothing_else() {
        let mut facts = github_facts("OPEN");
        facts.viewer_permission = Some("WRITE");
        assert!(github_capabilities(facts).can_rerun_checks);
        facts.viewer_permission = Some("READ");
        assert!(!github_capabilities(facts).can_rerun_checks);
        facts.viewer_permission = None;
        assert!(!github_capabilities(facts).can_rerun_checks);

        let mut facts = gitlab_facts("opened");
        facts.can_update_pipeline = Some(true);
        assert!(gitlab_capabilities(facts).can_rerun_checks);
        facts.can_update_pipeline = Some(false);
        assert!(!gitlab_capabilities(facts).can_rerun_checks);
        // A baseline answer does not report it: not offered (spec §6).
        facts.can_update_pipeline = None;
        assert!(!gitlab_capabilities(facts).can_rerun_checks);
    }

    fn github_facts(state: &'static str) -> GitHubFacts<'static> {
        GitHubFacts {
            state,
            locked: false,
            viewer_did_author: false,
            viewer_can_update: true,
            viewer_can_close: true,
            viewer_can_reopen: true,
            viewer_permission: None,
        }
    }

    /// A merged pull request is finished: nothing about its state, its draft
    /// flag or a review applies, whatever the viewer could otherwise do.
    #[test]
    fn a_merged_github_pull_request_offers_no_state_change_review_or_draft_toggle() {
        let caps = github_capabilities(github_facts("MERGED"));
        assert!(!caps.can_change_state);
        assert!(!caps.can_toggle_draft);
        assert!(!caps.can_approve);
        assert!(!caps.can_request_changes);
        assert!(caps.can_edit, "the title and description of a merged pull request can still be fixed");
        assert!(caps.can_comment);
    }

    #[test]
    fn nobody_reviews_their_own_github_pull_request() {
        let caps = github_capabilities(GitHubFacts {
            viewer_did_author: true,
            ..github_facts("OPEN")
        });
        assert!(!caps.can_approve);
        assert!(!caps.can_request_changes);
        assert!(caps.can_toggle_draft && caps.can_change_state);
    }

    #[test]
    fn a_locked_github_conversation_takes_the_composer_away_and_nothing_else() {
        let caps = github_capabilities(GitHubFacts {
            locked: true,
            ..github_facts("OPEN")
        });
        assert!(!caps.can_comment);
        assert!(caps.can_approve && caps.can_edit);
    }

    #[test]
    fn a_closed_github_pull_request_can_be_reopened_only_when_the_forge_says_so() {
        let allowed = github_capabilities(github_facts("CLOSED"));
        assert!(allowed.can_change_state);
        assert!(!allowed.can_toggle_draft && !allowed.can_approve);
        let refused = github_capabilities(GitHubFacts {
            viewer_can_reopen: false,
            ..github_facts("CLOSED")
        });
        assert!(!refused.can_change_state, "closed by a maintainer, the author cannot reopen it");
    }

    #[test]
    fn an_open_github_pull_request_is_closed_by_whoever_the_forge_allows() {
        let refused = github_capabilities(GitHubFacts {
            viewer_can_close: false,
            ..github_facts("OPEN")
        });
        assert!(!refused.can_change_state);
    }

    #[test]
    fn a_github_pull_request_the_viewer_may_not_update_offers_a_review_and_a_comment_only() {
        let caps = github_capabilities(GitHubFacts {
            viewer_can_update: false,
            viewer_can_close: false,
            viewer_can_reopen: false,
            ..github_facts("OPEN")
        });
        assert!(!caps.can_edit && !caps.can_toggle_draft && !caps.can_change_state);
        assert!(caps.can_comment && caps.can_approve && caps.can_request_changes);
    }

    #[test]
    fn an_unknown_github_state_is_treated_as_finished() {
        let caps = github_capabilities(github_facts("SOMETHING_NEW"));
        assert!(!caps.can_change_state && !caps.can_toggle_draft && !caps.can_approve);
        assert!(caps.can_comment);
    }

    fn gitlab_facts(state: &'static str) -> GitLabFacts<'static> {
        GitLabFacts {
            state,
            locked: false,
            can_create_note: Some(true),
            can_update: Some(true),
            can_approve: Some(true),
            reports_review_state: true,
            can_update_pipeline: None,
        }
    }

    #[test]
    fn a_gitlab_that_reports_nothing_offers_a_comment_and_nothing_else() {
        let caps = gitlab_capabilities(GitLabFacts {
            state: "opened",
            locked: false,
            can_create_note: None,
            can_update: None,
            can_approve: None,
            reports_review_state: false,
            can_update_pipeline: None,
        });
        assert_eq!(
            caps,
            Capabilities {
                can_comment: true,
                ..Capabilities::default()
            }
        );
    }

    #[test]
    fn a_gitlab_user_who_may_not_create_notes_cannot_comment() {
        let caps = gitlab_capabilities(GitLabFacts {
            can_create_note: Some(false),
            ..gitlab_facts("opened")
        });
        assert!(!caps.can_comment);
        let locked = gitlab_capabilities(GitLabFacts {
            locked: true,
            ..gitlab_facts("opened")
        });
        assert!(!locked.can_comment);
    }

    #[test]
    fn requesting_changes_needs_a_server_that_reports_review_states() {
        let old = gitlab_capabilities(GitLabFacts {
            reports_review_state: false,
            ..gitlab_facts("opened")
        });
        assert!(old.can_approve && !old.can_request_changes);
        assert!(gitlab_capabilities(gitlab_facts("opened")).can_request_changes);
    }

    #[test]
    fn a_merged_or_closed_gitlab_merge_request_offers_no_review_and_no_draft_toggle() {
        for state in ["merged", "closed", "locked", "something_new"] {
            let caps = gitlab_capabilities(gitlab_facts(state));
            assert!(!caps.can_approve && !caps.can_request_changes && !caps.can_toggle_draft, "{state}");
        }
        assert!(gitlab_capabilities(gitlab_facts("closed")).can_change_state, "a closed merge request can be reopened");
        assert!(!gitlab_capabilities(gitlab_facts("merged")).can_change_state);
        assert!(!gitlab_capabilities(gitlab_facts("locked")).can_change_state);
    }

    #[test]
    fn a_gitlab_user_who_may_not_update_edits_and_changes_nothing() {
        let caps = gitlab_capabilities(GitLabFacts {
            can_update: Some(false),
            ..gitlab_facts("opened")
        });
        assert!(!caps.can_edit && !caps.can_change_state && !caps.can_toggle_draft);
        assert!(caps.can_approve, "approving is its own permission");
    }

    fn gh() -> GitHubMergeFacts<'static> {
        GitHubMergeFacts {
            state: "OPEN",
            merge_state_status: Some("CLEAN"),
            review_decision: None,
            rollup: None,
            merge_commit_allowed: true,
            squash_merge_allowed: true,
            rebase_merge_allowed: true,
            auto_merge_allowed: true,
            viewer_can_enable_auto_merge: true,
            auto_merge_method: None,
            delete_branch_on_merge: false,
        }
    }

    #[test]
    fn a_clean_pull_request_is_ready_with_every_allowed_method() {
        let cap = github_merge(gh());
        assert_eq!(cap.verdict, MergeVerdict::Ready);
        assert_eq!(cap.methods.list(), vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]);
        assert_eq!(cap.default_method, Some(MergeMethod::Merge));
    }

    #[test]
    fn a_status_word_github_adds_tomorrow_is_blocked_with_its_own_word() {
        let cap = github_merge(GitHubMergeFacts { merge_state_status: Some("QUANTUM_FOAM"), ..gh() });
        assert_eq!(cap.verdict, MergeVerdict::Blocked(BlockReason::Other("QUANTUM_FOAM".into())));
    }

    #[test]
    fn a_missing_status_is_unreported_not_ready() {
        let cap = github_merge(GitHubMergeFacts { merge_state_status: None, ..gh() });
        assert_eq!(cap.verdict, MergeVerdict::Unreported);
    }

    #[test]
    fn blocked_is_told_apart_by_the_review_decision_and_the_rollup() {
        let blocked = |decision, rollup| {
            github_merge(GitHubMergeFacts {
                merge_state_status: Some("BLOCKED"),
                review_decision: decision,
                rollup,
                ..gh()
            })
            .verdict
        };
        assert_eq!(blocked(Some("REVIEW_REQUIRED"), None), MergeVerdict::Blocked(BlockReason::ReviewRequired));
        assert_eq!(blocked(Some("CHANGES_REQUESTED"), None), MergeVerdict::Blocked(BlockReason::ChangesRequested));
        assert_eq!(blocked(None, Some("PENDING")), MergeVerdict::WaitingOnChecks);
        assert_eq!(blocked(None, Some("EXPECTED")), MergeVerdict::WaitingOnChecks);
        assert_eq!(blocked(None, Some("FAILURE")), MergeVerdict::Blocked(BlockReason::ChecksFailing));
        assert_eq!(blocked(None, None), MergeVerdict::Blocked(BlockReason::Other("BLOCKED".into())));
    }

    #[test]
    fn dirty_behind_and_a_draft_name_their_reason() {
        let verdict = |status, state| {
            github_merge(GitHubMergeFacts { merge_state_status: Some(status), state, ..gh() }).verdict
        };
        assert_eq!(verdict("DIRTY", "OPEN"), MergeVerdict::Blocked(BlockReason::Conflicts));
        assert_eq!(verdict("BEHIND", "OPEN"), MergeVerdict::Blocked(BlockReason::Behind));
        assert_eq!(verdict("CLEAN", "DRAFT"), MergeVerdict::Blocked(BlockReason::Draft));
        assert_eq!(verdict("UNSTABLE", "OPEN"), MergeVerdict::Ready, "failing non-required checks do not block");
        assert_eq!(verdict("HAS_HOOKS", "OPEN"), MergeVerdict::Ready);
    }

    #[test]
    fn a_repository_that_allows_no_method_is_blocked_with_none_to_choose() {
        let cap = github_merge(GitHubMergeFacts {
            merge_commit_allowed: false,
            squash_merge_allowed: false,
            rebase_merge_allowed: false,
            ..gh()
        });
        assert!(cap.methods.is_empty());
        assert_eq!(cap.default_method, None);
        assert_eq!(cap.verdict, MergeVerdict::Blocked(BlockReason::NoMethod));
    }

    #[test]
    fn the_default_method_is_the_first_allowed_one() {
        let cap = github_merge(GitHubMergeFacts { merge_commit_allowed: false, rebase_merge_allowed: false, ..gh() });
        assert_eq!(cap.default_method, Some(MergeMethod::Squash));
    }

    #[test]
    fn auto_merge_needs_the_repository_and_the_viewer() {
        assert!(github_merge(gh()).can_auto_merge);
        assert!(!github_merge(GitHubMergeFacts { auto_merge_allowed: false, ..gh() }).can_auto_merge);
        assert!(!github_merge(GitHubMergeFacts { viewer_can_enable_auto_merge: false, ..gh() }).can_auto_merge);
        let on = github_merge(GitHubMergeFacts { auto_merge_method: Some("SQUASH"), ..gh() });
        assert_eq!(on.auto_merge_enabled, Some(MergeMethod::Squash));
    }

    #[test]
    fn an_unknown_auto_merge_method_still_reads_as_enabled() {
        let on = github_merge(GitHubMergeFacts { auto_merge_method: Some("TELEPORT"), ..gh() });
        assert_eq!(on.auto_merge_enabled, Some(MergeMethod::Merge), "enabled, shown with the default method rather than hidden");
    }

    fn gl() -> GitLabMergeFacts<'static> {
        GitLabMergeFacts {
            state: "opened",
            detailed_status: Some("MERGEABLE"),
            can_merge: Some(true),
            squash_read_only: Some(false),
            squash_on_merge: Some(false),
            auto_merge_enabled: Some(false),
            auto_merge_strategies: Some(&["merge_when_checks_pass"]),
            remove_source_branch: Some(false),
        }
    }

    #[test]
    fn a_forge_still_working_out_mergeability_is_checking_not_blocked() {
        for status in ["UNCHECKED", "CHECKING", "PREPARING", "APPROVALS_SYNCING"] {
            let cap = gitlab_merge(GitLabMergeFacts { detailed_status: Some(status), ..gl() });
            assert_eq!(cap.verdict, MergeVerdict::Checking, "{status}");
        }
        let cap = github_merge(GitHubMergeFacts { merge_state_status: Some("UNKNOWN"), ..gh() });
        assert_eq!(cap.verdict, MergeVerdict::Checking);
    }

    #[test]
    fn a_mergeable_merge_request_offers_merge_and_squash_never_rebase() {
        let cap = gitlab_merge(gl());
        assert_eq!(cap.verdict, MergeVerdict::Ready);
        assert_eq!(cap.methods.list(), vec![MergeMethod::Merge, MergeMethod::Squash]);
        assert!(cap.can_auto_merge);
    }

    #[test]
    fn a_read_only_squash_setting_removes_squash_or_forces_it() {
        let off = gitlab_merge(GitLabMergeFacts { squash_read_only: Some(true), squash_on_merge: Some(false), ..gl() });
        assert_eq!(off.methods.list(), vec![MergeMethod::Merge]);
        let forced = gitlab_merge(GitLabMergeFacts { squash_read_only: Some(true), squash_on_merge: Some(true), ..gl() });
        assert_eq!(forced.methods.list(), vec![MergeMethod::Squash], "a project that always squashes offers only squash");
        assert_eq!(forced.default_method, Some(MergeMethod::Squash));
    }

    #[test]
    fn gitlab_statuses_map_and_an_unknown_one_keeps_its_word() {
        let v = |word| gitlab_merge(GitLabMergeFacts { detailed_status: Some(word), ..gl() }).verdict;
        assert_eq!(v("CI_STILL_RUNNING"), MergeVerdict::WaitingOnChecks);
        assert_eq!(v("CI_MUST_PASS"), MergeVerdict::Blocked(BlockReason::ChecksFailing));
        assert_eq!(v("NOT_APPROVED"), MergeVerdict::Blocked(BlockReason::ReviewRequired));
        assert_eq!(v("REQUESTED_CHANGES"), MergeVerdict::Blocked(BlockReason::ChangesRequested));
        assert_eq!(v("CONFLICT"), MergeVerdict::Blocked(BlockReason::Conflicts));
        assert_eq!(v("NEED_REBASE"), MergeVerdict::Blocked(BlockReason::Behind));
        assert_eq!(v("DRAFT_STATUS"), MergeVerdict::Blocked(BlockReason::Draft));
        assert_eq!(v("DISCUSSIONS_NOT_RESOLVED"), MergeVerdict::Blocked(BlockReason::Discussions));
        assert_eq!(
            v("SECURITY_POLICIES_VIOLATIONS"),
            MergeVerdict::Blocked(BlockReason::Other("SECURITY_POLICIES_VIOLATIONS".into()))
        );
        assert_eq!(v("A_NEW_WORD"), MergeVerdict::Blocked(BlockReason::Other("A_NEW_WORD".into())));
    }

    #[test]
    fn a_baseline_gitlab_answer_reports_no_merge_capability() {
        let cap = gitlab_merge(GitLabMergeFacts {
            detailed_status: None,
            can_merge: None,
            squash_read_only: None,
            squash_on_merge: None,
            auto_merge_enabled: None,
            auto_merge_strategies: None,
            remove_source_branch: None,
            ..gl()
        });
        assert_eq!(cap.verdict, MergeVerdict::Unreported);
        assert!(cap.methods.is_empty() && !cap.can_auto_merge);
    }

    #[test]
    fn a_user_who_cannot_merge_on_gitlab_is_blocked() {
        let cap = gitlab_merge(GitLabMergeFacts { can_merge: Some(false), ..gl() });
        assert_eq!(cap.verdict, MergeVerdict::Blocked(BlockReason::Other("cannot merge".into())));
    }

    #[test]
    fn a_block_reason_reads_as_words() {
        assert_eq!(BlockReason::Other("SECURITY_POLICIES_VIOLATIONS".into()).text(), "security policies violations");
        assert_eq!(BlockReason::ReviewRequired.text(), "a review is required");
    }
}

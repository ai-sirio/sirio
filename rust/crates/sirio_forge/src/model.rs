//! What Sirio knows about a change request, whichever forge it came from.
//! Timestamps are Unix seconds; each forge's spellings are translated in
//! `mapping`, never here.

use serde::{Deserialize, Serialize};

/// A forge flavour. Serialised lower-case (`"github"`, `"gitlab"`), which is
/// how Settings and the session store spell it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Forge {
    GitHub,
    GitLab,
}

impl Forge {
    /// "GitHub", "GitLab".
    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
        }
    }

    /// The forge's own noun: "PR" on GitHub, "MR" on GitLab.
    pub fn change_noun(self) -> &'static str {
        match self {
            Self::GitHub => "PR",
            Self::GitLab => "MR",
        }
    }

    /// The sigil the forge writes before a number: `#578`, `!231`.
    pub fn sigil(self) -> char {
        match self {
            Self::GitHub => '#',
            Self::GitLab => '!',
        }
    }
}

/// The stable identity of one change request: what a tab is keyed by and
/// what the session store keeps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeRef {
    pub forge: Forge,
    pub host: String,
    pub project: String,
    pub number: u64,
}

impl ChangeRef {
    /// `#578` or `!231`.
    pub fn label(&self) -> String {
        format!("{}{}", self.forge.sigil(), self.number)
    }

    /// The ref the forge keeps for this change request's head, forks
    /// included: `refs/pull/N/head` on GitHub, `refs/merge-requests/N/head`
    /// on GitLab.
    pub fn head_ref(&self) -> String {
        match self.forge {
            Forge::GitHub => format!("refs/pull/{}/head", self.number),
            Forge::GitLab => format!("refs/merge-requests/{}/head", self.number),
        }
    }
}

/// The pair of commits a change request's diff is taken between, as the
/// forge reports them (spec §3). The diff is `base...head`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revisions {
    pub base_sha: String,
    pub head_sha: String,
    /// GitLab's `diffRefs.startSha`, the target's tip when the diff was
    /// computed. Unused by B1; read now because B3's line positions need it.
    pub start_sha: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    Draft,
    Open,
    Merged,
    Closed,
}

/// Finished jobs out of all jobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

/// The CI verdict for a change request's head. `Running(None)` is a run in
/// progress whose counts the forge did not give (a GitLab on the baseline
/// queries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiState {
    NoChecks,
    Running(Option<Progress>),
    Passed,
    Failed,
    Canceled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewState {
    Approved { count: u32 },
    ChangesRequested,
    ReviewRequired,
    None,
}

/// One reviewer's latest word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOutcome {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Requested,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reviewer {
    /// What the reviewer mutation names: a GitHub node id, a GitLab
    /// username; `None` for a team or an id the forge did not give.
    pub id: Option<String>,
    pub login: String,
    pub outcome: ReviewOutcome,
}

/// One list row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSummary {
    pub reference: ChangeRef,
    pub title: String,
    pub author: String,
    pub state: ChangeState,
    pub ci: CiState,
    pub review: ReviewState,
    /// A review from the signed-in user is still pending.
    pub review_requested_from_me: bool,
    pub comments: u32,
    pub source_branch: String,
    pub target_branch: String,
    /// Who owns the source branch: GitHub's head repository owner, GitLab's
    /// source project path. `None` when the forge did not say — a deleted
    /// fork.
    pub source_owner: Option<String>,
    pub updated_at: Option<i64>,
    pub web_url: String,
}

/// Where the next page of a list starts. One slot per connection the list
/// is drawn from (two for a union such as *Mine*); `None` in a slot means
/// that connection is exhausted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageCursor {
    pub(crate) slots: Vec<Option<String>>,
}

impl PageCursor {
    pub(crate) fn slot(&self, index: usize) -> Option<&str> {
        self.slots.get(index).and_then(|slot| slot.as_deref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangePage {
    pub items: Vec<ChangeSummary>,
    pub next: Option<PageCursor>,
}

/// The four filters of the list (spec §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Mine,
    ToReview,
    AllOpen,
    ClosedAndMerged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListQuery {
    pub filter: Filter,
    /// Server-side search text; `None` or blank means no search.
    pub search: Option<String>,
}

impl ListQuery {
    pub(crate) fn search_text(&self) -> Option<&str> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineComment {
    pub author: String,
    pub path: String,
    pub line: Option<u32>,
    pub body: String,
    pub at: Option<i64>,
}

/// Which side of a diff a thread or a line sits on: the base's lines
/// (`Old`, GitHub `LEFT`) or the head's (`New`, GitHub `RIGHT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Old,
    New,
}

/// One review discussion anchored in a change request's diff (spec §3, §4):
/// a GitHub review thread or a GitLab diff discussion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThread {
    /// GitHub's `PullRequestReviewThread` node id, or GitLab's discussion id.
    pub id: String,
    pub path: String,
    pub side: Side,
    /// The anchored line on `side`; for an outdated GitHub thread, the line
    /// it was written on (`originalLine`). `None` for a file-level thread.
    pub line: Option<u32>,
    /// The first line of a range; GitLab's GraphQL does not report one.
    pub start_line: Option<u32>,
    /// The forge says the anchor no longer maps onto the current diff.
    pub outdated: bool,
    pub resolved: bool,
    pub resolved_by: Option<String>,
    /// The code the thread quoted, in diff form (`+`/`-`/` ` prefixes).
    pub diff_hunk: Option<String>,
    pub can_reply: bool,
    /// May resolve it, or, when resolved, unresolve it.
    pub can_resolve: bool,
    /// About the whole file, not a line: never drawn in the diff.
    pub file_level: bool,
    pub comments: Vec<ThreadComment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub at: Option<i64>,
    /// Filled from slice B3b on; `None` throughout B3a.
    pub edit: Option<CommentRef>,
    /// Part of the viewer's unsubmitted review (spec §3 "Draft").
    pub pending: bool,
}

/// What a drawn diff line is, for a new comment's position (spec §3 "Anchor").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

/// One line of a new comment's anchor, in GitLab's diff counters: `old` and
/// `new` are where the line is, and for a side the line is not on, where
/// that side's next line would be (an added line's `old` is the next base
/// line). GitHub reads only the number on the comment's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorLine {
    pub kind: LineKind,
    pub old: u32,
    pub new: u32,
}

impl AnchorLine {
    /// The line's number on `side`, when the line exists there.
    pub fn on(self, side: Side) -> Option<u32> {
        match (side, self.kind) {
            (Side::New, LineKind::Removed) | (Side::Old, LineKind::Added) => None,
            (Side::New, _) => Some(self.new),
            (Side::Old, _) => Some(self.old),
        }
    }
}

/// Where a new comment goes: one side, its last line and, for a range, its
/// first. The path is the file's path at the head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineAnchor {
    pub path: String,
    pub side: Side,
    pub line: AnchorLine,
    pub start: Option<AnchorLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    CommitsPushed {
        count: u32,
    },
    ReviewRequested {
        reviewer: String,
    },
    Merged,
    Closed,
    Reopened,
    ReadyForReview,
    ConvertedToDraft,
    /// Anything else the forge recorded, as its own first line.
    Other(String),
}

/// Which of the forge's two kinds of text an edit of one's own words
/// changes. GitHub edits a comment and a review's body through different
/// mutations; GitLab has notes only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Comment,
    Review,
    /// A comment in a review thread (GitHub's pull request review comment).
    ReviewComment,
    /// A comment of the viewer's unsubmitted review: a GitHub pending review
    /// comment, or a GitLab draft note (its REST id).
    Draft,
}

/// What an edit points at. A timeline entry carries one only where the
/// forge says the signed-in user may edit it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentRef {
    pub id: String,
    pub kind: CommentKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineItem {
    Comment {
        author: String,
        body: String,
        at: Option<i64>,
        edit: Option<CommentRef>,
    },
    Review {
        author: String,
        outcome: ReviewOutcome,
        body: String,
        at: Option<i64>,
        line_comments: Vec<LineComment>,
        edit: Option<CommentRef>,
    },
    LineComment(LineComment),
    Event {
        actor: Option<String>,
        kind: EventKind,
        at: Option<i64>,
    },
}

/// What the forge says the signed-in user may do to one change request
/// now (spec §6). A capability the forge did not report is `false` — a
/// button Sirio is unsure of is not offered — except commenting, which is
/// offered unless the conversation is locked. The forge's refusal is shown
/// either way: this is a courtesy, the server has the last word.
/// How a change request's commits land on its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "Merge commit",
            Self::Squash => "Squash and merge",
            Self::Rebase => "Rebase and merge",
        }
    }

    /// A content-free word for reports and the control socket.
    pub fn word(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

/// The merge methods a repository allows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeMethods {
    pub merge: bool,
    pub squash: bool,
    pub rebase: bool,
}

impl MergeMethods {
    pub fn contains(self, method: MergeMethod) -> bool {
        match method {
            MergeMethod::Merge => self.merge,
            MergeMethod::Squash => self.squash,
            MergeMethod::Rebase => self.rebase,
        }
    }

    /// In a fixed order: merge, squash, rebase.
    pub fn list(self) -> Vec<MergeMethod> {
        [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]
            .into_iter()
            .filter(|method| self.contains(*method))
            .collect()
    }

    pub fn is_empty(self) -> bool {
        !(self.merge || self.squash || self.rebase)
    }
}

/// Why a change request cannot be merged now, as the forge put it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockReason {
    Conflicts,
    ReviewRequired,
    ChangesRequested,
    ChecksFailing,
    Behind,
    Draft,
    Discussions,
    /// The repository allows no merge method Sirio supports.
    NoMethod,
    /// The forge's own word, for a status Sirio does not know.
    Other(String),
}

impl BlockReason {
    pub fn text(&self) -> String {
        match self {
            Self::Conflicts => "merge conflicts".to_string(),
            Self::ReviewRequired => "a review is required".to_string(),
            Self::ChangesRequested => "changes were requested".to_string(),
            Self::ChecksFailing => "checks are failing".to_string(),
            Self::Behind => "the branch is behind its target".to_string(),
            Self::Draft => "it is a draft".to_string(),
            Self::Discussions => "discussions are not resolved".to_string(),
            Self::NoMethod => "the repository allows no merge method Sirio supports".to_string(),
            Self::Other(word) => word.to_lowercase().replace('_', " "),
        }
    }
}

/// Whether the forge would merge now. `Unreported` is "the forge did not
/// say" — nothing is offered for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MergeVerdict {
    #[default]
    Unreported,
    Ready,
    WaitingOnChecks,
    /// The forge has not yet worked out whether it can merge (right after
    /// a push): nothing is offered, and it is worth asking again shortly.
    Checking,
    Blocked(BlockReason),
}

/// What the merge strip offers (spec §6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeCapability {
    pub verdict: MergeVerdict,
    pub methods: MergeMethods,
    pub default_method: Option<MergeMethod>,
    pub can_auto_merge: bool,
    /// Set when an auto-merge is enabled, with the method it will use.
    pub auto_merge_enabled: Option<MergeMethod>,
    pub delete_branch_default: bool,
}

/// A label on a change request; `id` is what its forge's mutation names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// A reviewer or a label found by a search, with the id its mutation needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub label: String,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub can_comment: bool,
    pub can_approve: bool,
    pub can_request_changes: bool,
    /// Title, description and target branch.
    pub can_edit: bool,
    /// Close an open change request, or reopen a closed one — whichever
    /// applies to its state.
    pub can_change_state: bool,
    /// Draft ↔ ready, on an open change request.
    pub can_toggle_draft: bool,
    pub can_edit_reviewers: bool,
    pub can_edit_labels: bool,
    /// Re-run a failed job or a run's failed jobs (spec §15.1).
    pub can_rerun_checks: bool,
    pub merge: MergeCapability,
}

/// The viewer's unsubmitted review (spec §3 "Draft"): GitHub's pending
/// review, or GitLab's draft notes, which have no review id of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub id: Option<String>,
    pub comments: u32,
}

/// The detail tab's header and Conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeHeader {
    pub summary: ChangeSummary,
    pub capabilities: Capabilities,
    pub body: String,
    pub reviewers: Vec<Reviewer>,
    pub labels: Vec<Label>,
    /// `None` where the forge did not say (a GitLab on the baseline query).
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub changed_files: Option<u32>,
    pub commit_count: Option<u32>,
    pub timeline: Vec<TimelineItem>,
    /// The timeline is the newest 100 items (`last: 100`); the forge has
    /// older ones. The tab says so and links to the forge rather than cutting
    /// silently.
    pub timeline_truncated: bool,
    /// `None` where the forge did not report them, or reported them
    /// malformed: *Files* then stays on the forge's list (spec §7.1).
    pub revisions: Option<Revisions>,
    /// The viewer's own review in progress, read with the header (B3c).
    pub draft: Option<Draft>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub title: String,
    pub author: String,
    pub at: Option<i64>,
    pub web_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Canceled,
    Skipped,
    Neutral,
}

/// What makes a check a CI job Sirio can read and re-run: a GitHub Actions
/// job or a GitLab CI job (spec §7.3, §15.1). A third-party check run and a
/// GitHub `StatusContext` have none, and keep opening their page.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CheckJob {
    /// GitHub: the check run's `databaseId`, which is the Actions job id.
    /// GitLab: the number of `gid://gitlab/Ci::Build/N`.
    pub job_id: u64,
    /// The GitHub workflow run, or the GitLab pipeline, it ran in.
    pub run_id: Option<u64>,
    /// The forge would take a re-run of it now: on GitHub its run has
    /// finished, on GitLab the job says `retryable`.
    pub retryable: bool,
}

/// The tail of a CI job's log, as the forge served it (spec §7.4, §15.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    /// Escape sequences and all; at most `LOG_TAIL_BYTES`, starting at a line.
    pub bytes: Vec<u8>,
    /// Bytes dropped from the start to keep the tail; 0 when it is whole.
    pub dropped: u64,
    /// The job has finished: its log will not grow.
    pub complete: bool,
    /// The forge serves a log for it yet. A GitHub job's log appears when
    /// the job ends; a manual GitLab job that never ran has none.
    pub published: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    /// The workflow (GitHub) or stage (GitLab) it belongs to.
    pub group: Option<String>,
    pub duration_secs: Option<u64>,
    pub url: Option<String>,
    /// `Some` for a GitHub Actions job and a GitLab CI job only.
    pub job: Option<CheckJob>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// One changed file. GitLab's `diffStats` does not say how a file changed,
/// so `kind` is `None` there. Neither forge's GraphQL gives a previous path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: Option<FileChangeKind>,
    pub additions: u32,
    pub deletions: u32,
}

/// A detail list, and whether the forge had more than Sirio fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub truncated: bool,
}

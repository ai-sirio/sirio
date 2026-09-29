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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
}

/// The detail tab's header and Conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeHeader {
    pub summary: ChangeSummary,
    pub capabilities: Capabilities,
    pub body: String,
    pub reviewers: Vec<Reviewer>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    /// The workflow (GitHub) or stage (GitLab) it belongs to.
    pub group: Option<String>,
    pub duration_secs: Option<u64>,
    pub url: Option<String>,
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

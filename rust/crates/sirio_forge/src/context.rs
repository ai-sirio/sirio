//! What a hand-off reads from the forge for one purpose (change requests
//! C2, spec C §6): the header, and the threads, the failed jobs and their
//! logs, or the commits and files the purpose renders. Reads only.

use crate::{ChangeHeader, Check, CheckStatus, CommitSummary, FileChange, Log, ReviewThread};

/// The purposes a hand-off can start from. `word` is what the hand-off file
/// and the control socket call it; `label` is what the menu says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    Comments,
    Ci,
    Review,
    Resume,
}

impl Purpose {
    pub const ALL: [Purpose; 4] = [Purpose::Comments, Purpose::Ci, Purpose::Review, Purpose::Resume];

    pub fn word(self) -> &'static str {
        match self {
            Self::Comments => "comments",
            Self::Ci => "ci",
            Self::Review => "review",
            Self::Resume => "resume",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|purpose| purpose.word() == word)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Comments => "Fix review comments",
            Self::Ci => "Fix failing CI",
            Self::Review => "Review",
            Self::Resume => "Resume",
        }
    }
}

/// What the purpose is narrowed to: one review thread, one failed CI job, or
/// the whole change request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Whole,
    Thread(String),
    Job(u64),
}

impl Scope {
    pub fn word(&self) -> String {
        match self {
            Self::Whole => String::new(),
            Self::Thread(id) => format!("thread:{id}"),
            Self::Job(job) => format!("job:{job}"),
        }
    }
}

/// A failed check and its job's log. `log` is `None` when the check is not a
/// CI job Sirio can read; `Some(Err)` keeps the reason the log is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedJob {
    pub check: Check,
    pub log: Option<Result<Log, String>>,
}

/// Everything a hand-off for one purpose reads from the forge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub header: ChangeHeader,
    /// The signed-in user's login, when the forge said.
    pub viewer: Option<String>,
    pub threads: Vec<ReviewThread>,
    pub threads_truncated: bool,
    pub failed: Vec<FailedJob>,
    pub commits: Vec<CommitSummary>,
    pub files: Vec<FileChange>,
}

impl Context {
    /// Whether the signed-in user wrote the change request; `None` when the
    /// viewer is unknown.
    pub fn viewer_is_author(&self) -> Option<bool> {
        self.viewer
            .as_deref()
            .map(|viewer| viewer.eq_ignore_ascii_case(&self.header.summary.author))
    }
}

/// The failed checks in `scope`. A job scope keeps that job alone; a failed
/// check with no job (a third-party check) is kept for a whole-request scope.
pub fn failed_checks(checks: &[Check], scope: &Scope) -> Vec<Check> {
    checks
        .iter()
        .filter(|check| check.status == CheckStatus::Failed)
        .filter(|check| match scope {
            Scope::Job(job_id) => check.job.as_ref().is_some_and(|job| job.job_id == *job_id),
            Scope::Whole | Scope::Thread(_) => true,
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Check, CheckJob, CheckStatus};

    fn check(name: &str, status: CheckStatus, job: Option<u64>) -> Check {
        Check {
            name: name.to_string(),
            status,
            group: Some("CI".to_string()),
            duration_secs: None,
            url: Some(format!("https://forge.test/{name}")),
            job: job.map(|job_id| CheckJob { job_id, run_id: Some(1), retryable: true }),
        }
    }

    fn names(checks: &[Check]) -> Vec<&str> {
        checks.iter().map(|check| check.name.as_str()).collect()
    }

    fn sample() -> Vec<Check> {
        vec![
            check("build", CheckStatus::Passed, Some(1)),
            check("test", CheckStatus::Failed, Some(2)),
            check("lint", CheckStatus::Running, Some(3)),
            check("deploy", CheckStatus::Failed, Some(4)),
            check("codecov", CheckStatus::Failed, None),
        ]
    }

    #[test]
    fn only_failed_checks_are_kept_third_party_ones_included() {
        assert_eq!(names(&failed_checks(&sample(), &Scope::Whole)), ["test", "deploy", "codecov"]);
    }

    #[test]
    fn a_job_scope_keeps_that_job_alone() {
        assert_eq!(names(&failed_checks(&sample(), &Scope::Job(4))), ["deploy"]);
    }

    #[test]
    fn a_job_scope_naming_no_failed_job_keeps_nothing() {
        assert!(failed_checks(&sample(), &Scope::Job(1)).is_empty(), "job 1 passed");
        assert!(failed_checks(&sample(), &Scope::Job(99)).is_empty());
    }

    #[test]
    fn purposes_round_trip_their_words() {
        for purpose in Purpose::ALL {
            assert_eq!(Purpose::parse(purpose.word()), Some(purpose));
        }
        assert_eq!(Purpose::parse("fix"), None);
    }
}

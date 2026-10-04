//! The one door for a write (spec §4, §5): [`ForgeClient::act`]. Everything
//! that must hold for every action holds here once — the check against what
//! the forge says the user may do, before anything goes on the wire.
//!
//! [`ForgeClient::act`]: crate::ForgeClient::act

use crate::error::ForgeError;
use crate::model::{Capabilities, ChangeState, CommentRef, Forge, MergeMethod, MergeVerdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
    Comment,
}

/// One write a user asks of a forge about one change request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Comment {
        body: String,
    },
    Review {
        verdict: ReviewVerdict,
        body: String,
    },
    Close,
    Reopen,
    MarkReady,
    ConvertToDraft,
    /// Only what changed: a `None` field is left as it is.
    Edit {
        title: Option<String>,
        body: Option<String>,
        target_branch: Option<String>,
    },
    EditComment {
        comment: CommentRef,
        body: String,
    },
    /// `expected_head` is the head the user saw; the forge refuses the merge
    /// if the branch moved since. `commit_*` are `None` where the method has
    /// no message (a rebase).
    Merge {
        method: MergeMethod,
        commit_title: Option<String>,
        commit_message: Option<String>,
        delete_branch: bool,
        when_checks_pass: bool,
        expected_head: String,
    },
    CancelAutoMerge,
    /// Ids as `Reviewer::id` and `Candidate::id` carry them.
    SetReviewers {
        add: Vec<String>,
        remove: Vec<String>,
    },
    SetLabels {
        add: Vec<String>,
        remove: Vec<String>,
    },
}

impl Action {
    /// A name with no content in it, for traces and for the tab's report.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Comment { .. } => "comment",
            Self::Review {
                verdict: ReviewVerdict::Approve,
                ..
            } => "approve",
            Self::Review {
                verdict: ReviewVerdict::RequestChanges,
                ..
            } => "request-changes",
            Self::Review {
                verdict: ReviewVerdict::Comment,
                ..
            } => "review-comment",
            Self::Close => "close",
            Self::Reopen => "reopen",
            Self::MarkReady => "ready",
            Self::ConvertToDraft => "draft",
            Self::Edit { .. } => "edit",
            Self::EditComment { .. } => "edit-comment",
            Self::Merge {
                when_checks_pass: true,
                ..
            } => "auto-merge",
            Self::Merge { .. } => "merge",
            Self::CancelAutoMerge => "cancel-auto-merge",
            Self::SetReviewers { .. } => "set-reviewers",
            Self::SetLabels { .. } => "set-labels",
        }
    }
}

/// What a successful action leaves to say: the write happened, but a second
/// step of it did not (approved, but the comment beside it was refused).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionOutcome {
    pub warning: Option<String>,
}

/// What one pre-flight read of a change request tells `act`: the id the
/// mutations name, and the facts the check reads. Read fresh for every
/// action, so a permission that changed since the tab drew is honoured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionContext {
    pub node_id: String,
    pub state: ChangeState,
    pub capabilities: Capabilities,
    /// The head as the forge has it now; `None` where it did not say.
    pub head_sha: Option<String>,
    pub head_ref_name: Option<String>,
    /// The head lives in a fork: its branch is not Sirio's to delete.
    pub cross_repository: bool,
    /// The users, teams and bots asked to review now, as their mutation
    /// names them.
    pub reviewer_ids: Vec<String>,
    pub team_ids: Vec<String>,
    pub bot_ids: Vec<String>,
    /// The kinds of reviewer asked now that the mutation cannot name (a
    /// GitHub mannequin or enterprise team): a write that replaces the set
    /// would drop them.
    pub unsendable_requests: Vec<String>,
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Refuses what the forge would refuse, before sending it. `Rejected`
/// carries the reason in the words the user reads.
pub(crate) fn check_action(
    host: &str,
    action: &Action,
    context: &ActionContext,
) -> Result<(), ForgeError> {
    let caps = &context.capabilities;
    let state = context.state;
    let refuse = |message: &str| {
        Err(ForgeError::Rejected {
            host: host.to_string(),
            message: message.to_string(),
        })
    };
    let open = matches!(state, ChangeState::Open | ChangeState::Draft);
    match action {
        Action::Comment { body }
        | Action::Review {
            verdict: ReviewVerdict::Comment,
            body,
        } => {
            if !caps.can_comment {
                return refuse("You cannot comment on this change request.");
            }
            if blank(body) {
                return refuse("A comment needs some text.");
            }
        }
        Action::Review {
            verdict: ReviewVerdict::Approve,
            ..
        } => {
            if !caps.can_approve {
                return refuse("You cannot approve this change request.");
            }
        }
        Action::Review {
            verdict: ReviewVerdict::RequestChanges,
            body,
        } => {
            if !caps.can_request_changes {
                return refuse("You cannot request changes on this change request.");
            }
            if blank(body) {
                return refuse("Requesting changes needs a reason.");
            }
        }
        Action::Close => {
            if !caps.can_change_state || !open {
                return refuse("This change request cannot be closed.");
            }
        }
        Action::Reopen => {
            if !caps.can_change_state || state != ChangeState::Closed {
                return refuse("This change request cannot be reopened.");
            }
        }
        Action::MarkReady => {
            if !caps.can_toggle_draft || state != ChangeState::Draft {
                return refuse("This change request is not a draft you can mark ready.");
            }
        }
        Action::ConvertToDraft => {
            if !caps.can_toggle_draft || state != ChangeState::Open {
                return refuse("This change request cannot be turned into a draft.");
            }
        }
        Action::Edit {
            title,
            body,
            target_branch,
        } => {
            if !caps.can_edit {
                return refuse("You cannot edit this change request.");
            }
            if title.is_none() && body.is_none() && target_branch.is_none() {
                return refuse("There is nothing to change.");
            }
            if title.as_deref().is_some_and(blank) {
                return refuse("The title cannot be empty.");
            }
            if target_branch.as_deref().is_some_and(blank) {
                return refuse("The target branch cannot be empty.");
            }
        }
        Action::EditComment { body, .. } => {
            if blank(body) {
                return refuse("A comment needs some text.");
            }
        }
        Action::Merge {
            method,
            commit_title,
            when_checks_pass,
            expected_head,
            ..
        } => {
            // First: a wording of the forge's refusal is not a contract.
            if let Some(head) = &context.head_sha
                && head != expected_head
            {
                return Err(ForgeError::HeadMoved {
                    host: host.to_string(),
                });
            }
            if state != ChangeState::Open {
                return refuse("This change request is not open to merge.");
            }
            let merge = &caps.merge;
            if !merge.methods.contains(*method) {
                return refuse("The repository does not allow that merge method.");
            }
            if commit_title.as_deref().is_some_and(blank) {
                return refuse("The commit title cannot be empty.");
            }
            match (&merge.verdict, *when_checks_pass) {
                (MergeVerdict::Ready, false) => {}
                (MergeVerdict::WaitingOnChecks, true) if merge.can_auto_merge => {}
                (MergeVerdict::WaitingOnChecks, true) => {
                    return refuse("Merging when checks pass is not available here.");
                }
                (MergeVerdict::WaitingOnChecks, false) => return refuse("Checks are still running."),
                (MergeVerdict::Ready, true) => {
                    return refuse("Nothing is waiting on checks; merge it now.");
                }
                (MergeVerdict::Blocked(reason), _) => {
                    return refuse(&format!("It cannot be merged yet: {}.", reason.text()));
                }
                (MergeVerdict::Unreported, _) => {
                    return refuse("This forge did not say whether it can be merged.");
                }
                (MergeVerdict::Checking, _) => {
                    return refuse("The forge is still working out whether it can be merged; try again in a moment.");
                }
            }
        }
        Action::CancelAutoMerge => {
            if caps.merge.auto_merge_enabled.is_none() {
                return refuse("No auto-merge is set.");
            }
        }
        Action::SetReviewers { add, remove } => {
            if !caps.can_edit_reviewers {
                return refuse("You cannot change the reviewers.");
            }
            if add.is_empty() && remove.is_empty() {
                return refuse("There is nothing to change.");
            }
            if !remove.is_empty() && !context.unsendable_requests.is_empty() {
                return refuse(&format!(
                    "Removing a reviewer here would also drop the review request of a {}; change it on the forge.",
                    context.unsendable_requests.join(", ")
                ));
            }
        }
        Action::SetLabels { add, remove } => {
            if !caps.can_edit_labels {
                return refuse("You cannot change the labels.");
            }
            if add.is_empty() && remove.is_empty() {
                return refuse("There is nothing to change.");
            }
        }
    }
    Ok(())
}

/// One GraphQL document Sirio writes with, for the live conformance test.
#[doc(hidden)]
pub struct LiveProbe {
    pub operation: &'static str,
    pub document: &'static str,
    pub variables: serde_json::Value,
}

/// Every document `act` sends and every pre-flight read it makes, each with
/// variables that change nothing anywhere: a mutation's input names an id no
/// forge holds (a GitHub node id nobody minted, GitLab's global id and iid
/// `0`, which no row has), and a read names a public change request. The live
/// test sends each as it is and requires the forge to accept the *document* —
/// a schema or input error would carry `extensions` — whatever it thinks of
/// the ids.
#[doc(hidden)]
pub fn live_probes(forge: Forge) -> Vec<LiveProbe> {
    match forge {
        Forge::GitHub => crate::github::live_probes(),
        Forge::GitLab => crate::gitlab::live_probes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockReason, CommentKind, MergeCapability, MergeMethod, MergeMethods, MergeVerdict};

    fn everything() -> Capabilities {
        Capabilities {
            can_comment: true,
            can_approve: true,
            can_request_changes: true,
            can_edit: true,
            can_change_state: true,
            can_toggle_draft: true,
            can_edit_reviewers: true,
            can_edit_labels: true,
            can_rerun_checks: false,
            merge: ready_merge(),
        }
    }

    fn ready_merge() -> MergeCapability {
        MergeCapability {
            verdict: MergeVerdict::Ready,
            methods: MergeMethods { merge: true, squash: true, rebase: false },
            default_method: Some(MergeMethod::Merge),
            can_auto_merge: true,
            auto_merge_enabled: None,
            delete_branch_default: false,
        }
    }

    fn merge(method: MergeMethod, when: bool) -> Action {
        Action::Merge {
            method,
            commit_title: Some("t".into()),
            commit_message: Some("m".into()),
            delete_branch: false,
            when_checks_pass: when,
            expected_head: "abc123".into(),
        }
    }

    fn with_head(head: Option<&str>, caps: Capabilities) -> ActionContext {
        ActionContext {
            head_sha: head.map(str::to_string),
            ..context(ChangeState::Open, caps)
        }
    }

    fn context(state: ChangeState, capabilities: Capabilities) -> ActionContext {
        ActionContext {
            node_id: "PR_1".to_string(),
            state,
            capabilities,
            head_sha: None,
            head_ref_name: None,
            cross_repository: false,
            reviewer_ids: Vec::new(),
            team_ids: Vec::new(),
            bot_ids: Vec::new(),
            unsendable_requests: Vec::new(),
        }
    }

    fn check(action: &Action, state: ChangeState, capabilities: Capabilities) -> Result<(), String> {
        check_action("ghe.test", action, &context(state, capabilities)).map_err(|error| match error {
            ForgeError::Rejected { message, .. } => message,
            other => panic!("a refusal is Rejected, not {other:?}"),
        })
    }

    fn comment(text: &str) -> Action {
        Action::Comment { body: text.to_string() }
    }

    fn review(verdict: ReviewVerdict, text: &str) -> Action {
        Action::Review { verdict, body: text.to_string() }
    }

    #[test]
    fn a_comment_needs_words() {
        for empty in ["", "   ", "\n\t \n"] {
            assert_eq!(check(&comment(empty), ChangeState::Open, everything()), Err("A comment needs some text.".to_string()), "{empty:?}");
            assert_eq!(check(&review(ReviewVerdict::Comment, empty), ChangeState::Open, everything()), Err("A comment needs some text.".to_string()));
        }
        assert!(check(&comment("LGTM"), ChangeState::Open, everything()).is_ok());
    }

    #[test]
    fn a_reason_is_needed_to_request_changes_and_not_to_approve() {
        assert!(check(&review(ReviewVerdict::RequestChanges, " "), ChangeState::Open, everything()).is_err());
        assert!(check(&review(ReviewVerdict::RequestChanges, "Please handle None."), ChangeState::Open, everything()).is_ok());
        assert!(check(&review(ReviewVerdict::Approve, ""), ChangeState::Open, everything()).is_ok());
    }

    #[test]
    fn nothing_is_sent_that_the_capabilities_deny() {
        let denied = Capabilities::default();
        for action in [
            comment("hi"),
            review(ReviewVerdict::Comment, "hi"),
            review(ReviewVerdict::Approve, ""),
            review(ReviewVerdict::RequestChanges, "no"),
            Action::Close,
            Action::Edit { title: Some("t".into()), body: None, target_branch: None },
        ] {
            assert!(check(&action, ChangeState::Open, denied.clone()).is_err(), "{action:?}");
        }
        for (action, state) in [
            (Action::Reopen, ChangeState::Closed),
            (Action::MarkReady, ChangeState::Draft),
            (Action::ConvertToDraft, ChangeState::Open),
        ] {
            assert!(check(&action, state, denied.clone()).is_err(), "{action:?}");
        }
    }

    #[test]
    fn a_state_change_needs_the_state_it_changes() {
        let ok = |action: Action, state| check(&action, state, everything()).is_ok();
        assert!(ok(Action::Close, ChangeState::Open));
        assert!(ok(Action::Close, ChangeState::Draft), "a draft can be closed");
        assert!(!ok(Action::Close, ChangeState::Closed));
        assert!(!ok(Action::Close, ChangeState::Merged));
        assert!(ok(Action::Reopen, ChangeState::Closed));
        assert!(!ok(Action::Reopen, ChangeState::Open));
        assert!(!ok(Action::Reopen, ChangeState::Merged), "a merged request is never reopened");
        assert!(ok(Action::MarkReady, ChangeState::Draft));
        assert!(!ok(Action::MarkReady, ChangeState::Open));
        assert!(ok(Action::ConvertToDraft, ChangeState::Open));
        assert!(!ok(Action::ConvertToDraft, ChangeState::Draft));
        assert!(!ok(Action::ConvertToDraft, ChangeState::Closed));
    }

    #[test]
    fn an_edit_must_change_something_and_never_empty_the_title() {
        let edit = |title: Option<&str>, body: Option<&str>, target: Option<&str>| Action::Edit {
            title: title.map(str::to_string),
            body: body.map(str::to_string),
            target_branch: target.map(str::to_string),
        };
        let go = |action: Action| check(&action, ChangeState::Open, everything());
        assert_eq!(go(edit(None, None, None)), Err("There is nothing to change.".to_string()));
        assert_eq!(go(edit(Some("  "), None, None)), Err("The title cannot be empty.".to_string()));
        assert_eq!(go(edit(None, None, Some(""))), Err("The target branch cannot be empty.".to_string()));
        assert!(go(edit(None, Some(""), None)).is_ok(), "a description may be emptied");
        assert!(go(edit(Some("New title"), Some("Body"), Some("develop"))).is_ok());
    }

    #[test]
    fn editing_a_comment_needs_words_and_no_capability() {
        let edit = |text: &str| Action::EditComment {
            comment: CommentRef { id: "IC_1".into(), kind: CommentKind::Comment },
            body: text.to_string(),
        };
        assert!(check(&edit(""), ChangeState::Open, Capabilities::default()).is_err());
        assert!(check(&edit("fixed"), ChangeState::Merged, Capabilities::default()).is_ok());
    }

    #[test]
    fn every_action_has_its_own_content_free_kind() {
        let kinds: Vec<&str> = [
            comment("x"),
            review(ReviewVerdict::Approve, ""),
            review(ReviewVerdict::RequestChanges, "x"),
            review(ReviewVerdict::Comment, "x"),
            Action::Close,
            Action::Reopen,
            Action::MarkReady,
            Action::ConvertToDraft,
            Action::Edit { title: None, body: None, target_branch: None },
            Action::EditComment { comment: CommentRef { id: "1".into(), kind: CommentKind::Review }, body: "x".into() },
            merge(MergeMethod::Merge, false),
            merge(MergeMethod::Merge, true),
            Action::CancelAutoMerge,
            Action::SetReviewers { add: vec!["u".into()], remove: vec![] },
            Action::SetLabels { add: vec!["l".into()], remove: vec![] },
        ]
        .iter()
        .map(Action::kind)
        .collect();
        let mut unique = kinds.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), kinds.len(), "{kinds:?}");
    }

    #[test]
    fn a_moved_head_is_refused_before_anything_is_sent() {
        let ctx = with_head(Some("def456"), everything());
        assert!(matches!(check_action("h", &merge(MergeMethod::Merge, false), &ctx), Err(ForgeError::HeadMoved { .. })));
        let same = with_head(Some("abc123"), everything());
        assert!(check_action("h", &merge(MergeMethod::Merge, false), &same).is_ok());
        let unknown = with_head(None, everything());
        assert!(
            check_action("h", &merge(MergeMethod::Merge, false), &unknown).is_ok(),
            "a forge that does not report the head leaves it to its own guard"
        );
    }

    #[test]
    fn a_blocked_change_request_is_never_merged() {
        let mut caps = everything();
        caps.merge.verdict = MergeVerdict::Blocked(BlockReason::ReviewRequired);
        let ctx = with_head(Some("abc123"), caps);
        let refused = check_action("h", &merge(MergeMethod::Merge, false), &ctx);
        assert!(
            matches!(refused, Err(ForgeError::Rejected { ref message, .. }) if message.contains("a review is required")),
            "{refused:?}"
        );
        assert!(check_action("h", &merge(MergeMethod::Merge, true), &ctx).is_err(), "auto-merge cannot get past a block either");
    }

    #[test]
    fn waiting_on_checks_allows_only_the_auto_merge_form() {
        let mut caps = everything();
        caps.merge.verdict = MergeVerdict::WaitingOnChecks;
        let ctx = with_head(Some("abc123"), caps.clone());
        assert!(check_action("h", &merge(MergeMethod::Merge, false), &ctx).is_err());
        assert!(check_action("h", &merge(MergeMethod::Merge, true), &ctx).is_ok());
        caps.merge.can_auto_merge = false;
        assert!(check_action("h", &merge(MergeMethod::Merge, true), &with_head(Some("abc123"), caps)).is_err());
    }

    #[test]
    fn an_unreported_verdict_sends_nothing() {
        let mut caps = everything();
        caps.merge = MergeCapability::default();
        assert!(check_action("h", &merge(MergeMethod::Merge, false), &with_head(None, caps)).is_err());
    }

    #[test]
    fn a_method_the_repository_does_not_allow_is_refused() {
        let ctx = with_head(Some("abc123"), everything());
        assert!(check_action("h", &merge(MergeMethod::Rebase, false), &ctx).is_err());
    }

    #[test]
    fn only_an_open_change_request_merges_and_an_empty_title_is_refused() {
        let ready = |state| ActionContext { head_sha: Some("abc123".into()), ..context(state, everything()) };
        for state in [ChangeState::Merged, ChangeState::Closed, ChangeState::Draft] {
            assert!(check_action("h", &merge(MergeMethod::Merge, false), &ready(state)).is_err(), "{state:?}");
        }
        let blank = Action::Merge {
            method: MergeMethod::Merge,
            commit_title: Some("  ".into()),
            commit_message: None,
            delete_branch: false,
            when_checks_pass: false,
            expected_head: "abc123".into(),
        };
        assert!(check_action("h", &blank, &ready(ChangeState::Open)).is_err());
    }

    #[test]
    fn cancelling_needs_an_auto_merge_to_cancel() {
        let mut caps = everything();
        assert!(check_action("h", &Action::CancelAutoMerge, &context(ChangeState::Open, caps.clone())).is_err());
        caps.merge.auto_merge_enabled = Some(MergeMethod::Squash);
        assert!(check_action("h", &Action::CancelAutoMerge, &context(ChangeState::Open, caps)).is_ok());
    }

    #[test]
    fn reviewers_and_labels_need_their_capability_and_a_change() {
        let set = |a: &[&str], r: &[&str]| Action::SetReviewers {
            add: a.iter().map(|s| s.to_string()).collect(),
            remove: r.iter().map(|s| s.to_string()).collect(),
        };
        let ok = context(ChangeState::Open, everything());
        assert!(check_action("h", &set(&["u1"], &[]), &ok).is_ok());
        assert!(check_action("h", &set(&[], &[]), &ok).is_err(), "nothing to change");
        let mut caps = everything();
        caps.can_edit_reviewers = false;
        assert!(check_action("h", &set(&["u1"], &[]), &context(ChangeState::Open, caps)).is_err());
        let labels = Action::SetLabels { add: vec!["l1".into()], remove: vec![] };
        let mut caps = everything();
        caps.can_edit_labels = false;
        assert!(check_action("h", &labels, &context(ChangeState::Open, caps)).is_err());
        assert!(check_action("h", &labels, &ok).is_ok());
    }

    #[test]
    fn a_removal_is_refused_beside_a_request_it_cannot_send_back() {
        let mannequin = ActionContext {
            unsendable_requests: vec!["Mannequin".into()],
            ..context(ChangeState::Open, everything())
        };
        let remove = Action::SetReviewers { add: vec![], remove: vec!["u1".into()] };
        let refused = check_action("h", &remove, &mannequin).expect_err("a removal would drop the mannequin");
        assert!(matches!(refused, ForgeError::Rejected { .. }), "{refused:?}");
        let add = Action::SetReviewers { add: vec!["u2".into()], remove: vec![] };
        assert!(check_action("h", &add, &mannequin).is_ok(), "adding leaves every request alone");
    }
}

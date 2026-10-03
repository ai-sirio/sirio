//! The one door for a write (spec §4, §5): [`ForgeClient::act`]. Everything
//! that must hold for every action holds here once — the check against what
//! the forge says the user may do, before anything goes on the wire.
//!
//! [`ForgeClient::act`]: crate::ForgeClient::act

use crate::error::ForgeError;
use crate::model::{Capabilities, ChangeState, CommentRef, Forge};

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
    use crate::model::{CommentKind, MergeCapability};

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
            merge: MergeCapability::default(),
        }
    }

    fn context(state: ChangeState, capabilities: Capabilities) -> ActionContext {
        ActionContext {
            node_id: "PR_1".to_string(),
            state,
            capabilities,
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
        ]
        .iter()
        .map(Action::kind)
        .collect();
        let mut unique = kinds.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), kinds.len(), "{kinds:?}");
    }
}

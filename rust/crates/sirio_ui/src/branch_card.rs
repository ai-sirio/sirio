//! The branch card's resolution (spec §6): which change request, if any, a
//! worktree's branch has. The right panel's card and the chat's session panel
//! read it through here, so the two can never disagree about a branch.

use std::path::Path;

use sirio_forge::{ChangeSummary, ForgeError};

use crate::forge_source::{ChangeRequestSource, ReadyConnection};

pub enum BranchCard {
    /// A detached HEAD with no link: nothing to show.
    Hidden,
    Found(ChangeSummary),
    /// The branch has no change request; `create_url` opens the forge's form.
    Missing { branch: String, create_url: String },
}

pub struct BranchCardRead {
    pub card: Result<BranchCard, ForgeError>,
    /// The error of reading the linked change request by number, when that
    /// failed and the branch lookup decided instead. It still has to reach
    /// the rate-limit pause.
    pub linked_error: Option<ForgeError>,
    /// The branch checked out when the card was read; `None` on a detached HEAD.
    pub branch: Option<String>,
}

/// Blocking: runs git and the forge, so call it from a background task.
///
/// The branch is read here, not taken from `ready`: `connect` read it once,
/// and the worktree may have switched since. A linked change request is read
/// by number first; if that fails, the branch lookup decides.
pub fn read_branch_card(
    source: &dyn ChangeRequestSource,
    worktree: &Path,
    ready: &ReadyConnection,
) -> BranchCardRead {
    let client = &ready.client;
    let branch = source.current_branch(worktree);
    let mut linked_error = None;
    match ready.linked.as_ref().map(|reference| client.summary(reference.number)) {
        Some(Ok(summary)) => {
            return BranchCardRead {
                card: Ok(BranchCard::Found(summary)),
                linked_error: None,
                branch,
            };
        }
        Some(Err(error)) => linked_error = Some(error),
        None => {}
    }
    let Some(found_branch) = branch.clone() else {
        // A detached HEAD has no branch to look up: only a link can have a
        // card, and its failure is reported.
        let card = match linked_error {
            Some(error) => Err(error),
            None => Ok(BranchCard::Hidden),
        };
        return BranchCardRead { card, linked_error: None, branch };
    };
    let card = client
        .for_branch(&found_branch, ready.source_owner.as_deref())
        .map(|found| match found {
            Some(summary) => BranchCard::Found(summary),
            None => BranchCard::Missing {
                create_url: client.creation_url(&found_branch),
                branch: found_branch,
            },
        });
    BranchCardRead { card, linked_error, branch }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use serde_json::json;
    use sirio_forge::{ChangeRef, Means};

    use super::*;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};
    use crate::forge_source::Connection;

    fn forge() -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestForBranch",
            testing::branch(vec![testing::summary(103, "This branch")]),
        );
        forge.answer(
            "ChangeRequestByNumber",
            json!({"data": {"repository": {"pullRequest": testing::summary(7, "The link")}}})
                .to_string(),
        );
        forge
    }

    fn ready(forge: Arc<CannedForge>, linked: Option<ChangeRef>) -> ReadyConnection {
        ReadyConnection {
            client: testing::github_client(forge),
            means: Means::Token,
            branch: None,
            source_owner: None,
            linked,
        }
    }

    fn read(branch: Option<&str>, ready: ReadyConnection) -> BranchCardRead {
        let source = FakeSource::ready(ready.client.clone(), branch);
        source.set(Connection::Ready(ready.clone()));
        read_branch_card(&*source, &PathBuf::from("/tmp/checkout"), &ready)
    }

    fn number(card: &Result<BranchCard, ForgeError>) -> Option<u64> {
        match card {
            Ok(BranchCard::Found(summary)) => Some(summary.reference.number),
            _ => None,
        }
    }

    #[test]
    fn a_link_on_the_current_branch_wins() {
        let forge = forge();
        let read = read(Some("feat/103"), ready(forge.clone(), Some(testing::reference(7))));
        assert_eq!(number(&read.card), Some(7), "the link is read, not the branch's own request");
        assert!(read.linked_error.is_none());
        assert_eq!(read.branch.as_deref(), Some("feat/103"));
        assert_eq!(forge.count("ChangeRequestForBranch"), 0, "the branch is not looked up");
    }

    #[test]
    fn a_failed_link_falls_back_to_the_branch_and_reports_its_error() {
        let forge = forge();
        forge.fail("ChangeRequestByNumber", 500);
        let read = read(Some("feat/103"), ready(forge, Some(testing::reference(7))));
        assert_eq!(number(&read.card), Some(103));
        assert!(read.linked_error.is_some(), "the link's failure still reaches the pause");
    }

    #[test]
    fn a_detached_head_without_a_link_has_no_card() {
        let forge = forge();
        let read = read(None, ready(forge.clone(), None));
        assert!(matches!(read.card, Ok(BranchCard::Hidden)));
        assert!(read.branch.is_none());
        assert_eq!(forge.count("ChangeRequestForBranch"), 0);
    }

    #[test]
    fn a_detached_head_with_a_failed_link_reports_the_failure() {
        let forge = forge();
        forge.fail("ChangeRequestByNumber", 500);
        let read = read(None, ready(forge, Some(testing::reference(7))));
        assert!(read.card.is_err());
        assert!(read.linked_error.is_none(), "it is the card's error, not reported twice");
    }

    #[test]
    fn a_branch_without_a_change_request_is_missing_with_its_create_url() {
        let forge = forge();
        forge.answer("ChangeRequestForBranch", testing::branch(vec![]));
        let ready = ready(forge, None);
        let expected = ready.client.creation_url("feat/new");
        let read = read(Some("feat/new"), ready);
        match read.card {
            Ok(BranchCard::Missing { branch, create_url }) => {
                assert_eq!(branch, "feat/new");
                assert_eq!(create_url, expected);
            }
            _ => panic!("expected a missing card"),
        }
    }
}

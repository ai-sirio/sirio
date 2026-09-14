//! The file editor's context-menu contract.
//!
//! The entries are a static table and the view only draws them, the shape
//! `sirio_terminal::context_menu` already uses. Facts the view cannot know on
//! its own — whether the file is in a git repository, whether that repository
//! has a GitHub remote, whether this worktree has an agent chat open — are
//! supplied by the workspace, because answering them costs a subprocess and a
//! walk of the open tabs.

const NO_SELECTION: &str = "Select some text first";
const NO_AGENT_CHAT: &str = "No agent chat open in this worktree";
const NO_GITHUB_REMOTE: &str = "This repository has no GitHub remote";
const NOT_IN_GIT: &str = "This file is not in a git repository";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileContextAction {
    SendToAgent,
    Cut,
    Copy,
    CopyAndTrim,
    Paste,
    RevealInFileManager,
    OpenInTerminal,
    OpenMarkdownPreview,
    CopyPermalink,
    ViewFileHistory,
}

/// `View` is served by `FileView` itself; `App` leaves as a `FileViewEvent`
/// for the workspace to apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileContextRoute {
    View,
    App,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContextItem {
    pub label: &'static str,
    pub action: FileContextAction,
    pub route: FileContextRoute,
    /// `Some(reason)` when this entry cannot currently be actuated. The render
    /// site must show the reason — not merely grey the row — and must attach
    /// no click handler, the contract the terminal menu already follows.
    pub disabled_reason: Option<String>,
    /// A separator is drawn wherever this changes between adjacent entries.
    pub group: u8,
}

/// Everything the table needs to decide what to hide and what to disable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileContextFacts {
    pub has_selection: bool,
    pub is_markdown: bool,
    pub in_git_repo: bool,
    pub has_github_remote: bool,
    pub has_agent_chat: bool,
}

const ITEMS: [FileContextItem; 10] = [
    FileContextItem {
        label: "Add to Agent Thread",
        action: FileContextAction::SendToAgent,
        route: FileContextRoute::App,
        disabled_reason: None,
        group: 0,
    },
    FileContextItem {
        label: "Cut",
        action: FileContextAction::Cut,
        route: FileContextRoute::View,
        disabled_reason: None,
        group: 1,
    },
    FileContextItem {
        label: "Copy",
        action: FileContextAction::Copy,
        route: FileContextRoute::View,
        disabled_reason: None,
        group: 1,
    },
    FileContextItem {
        label: "Copy and Trim",
        action: FileContextAction::CopyAndTrim,
        route: FileContextRoute::View,
        disabled_reason: None,
        group: 1,
    },
    FileContextItem {
        label: "Paste",
        action: FileContextAction::Paste,
        route: FileContextRoute::View,
        disabled_reason: None,
        group: 1,
    },
    FileContextItem {
        label: "Reveal in File Manager",
        action: FileContextAction::RevealInFileManager,
        route: FileContextRoute::App,
        disabled_reason: None,
        group: 2,
    },
    FileContextItem {
        label: "Open in Terminal",
        action: FileContextAction::OpenInTerminal,
        route: FileContextRoute::App,
        disabled_reason: None,
        group: 2,
    },
    FileContextItem {
        label: "Open Markdown Preview",
        action: FileContextAction::OpenMarkdownPreview,
        route: FileContextRoute::View,
        disabled_reason: None,
        group: 2,
    },
    FileContextItem {
        label: "Copy Permalink to Line",
        action: FileContextAction::CopyPermalink,
        route: FileContextRoute::App,
        disabled_reason: None,
        group: 3,
    },
    FileContextItem {
        label: "View File History",
        action: FileContextAction::ViewFileHistory,
        route: FileContextRoute::App,
        disabled_reason: None,
        group: 3,
    },
];

/// The entries to draw for `facts`, in order. An entry that is meaningless
/// here is absent; one that is merely unavailable is present with its reason.
pub fn items(facts: &FileContextFacts) -> Vec<FileContextItem> {
    ITEMS
        .iter()
        .cloned()
        .filter(|item| {
            item.action != FileContextAction::OpenMarkdownPreview || facts.is_markdown
        })
        .map(|mut item| {
            item.disabled_reason = disabled_reason(item.action, facts);
            item
        })
        .collect()
}

fn disabled_reason(action: FileContextAction, facts: &FileContextFacts) -> Option<String> {
    match action {
        FileContextAction::SendToAgent => {
            if !facts.has_selection {
                Some(NO_SELECTION.to_owned())
            } else if !facts.has_agent_chat {
                Some(NO_AGENT_CHAT.to_owned())
            } else {
                None
            }
        }
        FileContextAction::Cut | FileContextAction::Copy | FileContextAction::CopyAndTrim => {
            (!facts.has_selection).then(|| NO_SELECTION.to_owned())
        }
        FileContextAction::CopyPermalink => {
            if !facts.in_git_repo {
                Some(NOT_IN_GIT.to_owned())
            } else if !facts.has_github_remote {
                Some(NO_GITHUB_REMOTE.to_owned())
            } else {
                None
            }
        }
        FileContextAction::ViewFileHistory => {
            (!facts.in_git_repo).then(|| NOT_IN_GIT.to_owned())
        }
        FileContextAction::Paste
        | FileContextAction::RevealInFileManager
        | FileContextAction::OpenInTerminal
        | FileContextAction::OpenMarkdownPreview => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_true() -> FileContextFacts {
        FileContextFacts {
            has_selection: true,
            is_markdown: true,
            in_git_repo: true,
            has_github_remote: true,
            has_agent_chat: true,
        }
    }

    #[test]
    fn every_entry_is_offered_when_nothing_is_missing() {
        let labels: Vec<&str> = items(&all_true()).iter().map(|item| item.label).collect();
        assert_eq!(
            labels,
            vec![
                "Add to Agent Thread",
                "Cut",
                "Copy",
                "Copy and Trim",
                "Paste",
                "Reveal in File Manager",
                "Open in Terminal",
                "Open Markdown Preview",
                "Copy Permalink to Line",
                "View File History",
            ]
        );
        assert!(items(&all_true()).iter().all(|item| item.disabled_reason.is_none()));
    }

    #[test]
    fn markdown_preview_is_hidden_on_a_source_file() {
        let facts = FileContextFacts { is_markdown: false, ..all_true() };
        assert!(
            !items(&facts).iter().any(|item| item.action == FileContextAction::OpenMarkdownPreview),
            "a non-Markdown file must not offer the preview at all"
        );
    }

    #[test]
    fn without_a_selection_the_text_entries_say_why() {
        let facts = FileContextFacts { has_selection: false, ..all_true() };
        let items = items(&facts);
        for action in [
            FileContextAction::Cut,
            FileContextAction::Copy,
            FileContextAction::CopyAndTrim,
            FileContextAction::SendToAgent,
        ] {
            let item = items.iter().find(|item| item.action == action).expect("entry present");
            assert_eq!(item.disabled_reason.as_deref(), Some("Select some text first"));
        }
        let paste = items.iter().find(|item| item.action == FileContextAction::Paste).unwrap();
        assert!(paste.disabled_reason.is_none(), "Paste does not need a selection");
    }

    #[test]
    fn a_missing_chat_disables_only_the_agent_entry() {
        let facts = FileContextFacts { has_agent_chat: false, ..all_true() };
        let items = items(&facts);
        let agent = items.iter().find(|item| item.action == FileContextAction::SendToAgent).unwrap();
        assert_eq!(
            agent.disabled_reason.as_deref(),
            Some("No agent chat open in this worktree")
        );
        assert_eq!(items.iter().filter(|item| item.disabled_reason.is_some()).count(), 1);
    }

    #[test]
    fn a_non_github_remote_disables_the_permalink_but_not_the_history() {
        let facts = FileContextFacts { has_github_remote: false, ..all_true() };
        let items = items(&facts);
        let permalink = items.iter().find(|i| i.action == FileContextAction::CopyPermalink).unwrap();
        assert_eq!(
            permalink.disabled_reason.as_deref(),
            Some("This repository has no GitHub remote")
        );
        let history = items.iter().find(|i| i.action == FileContextAction::ViewFileHistory).unwrap();
        assert!(history.disabled_reason.is_none());
    }

    #[test]
    fn outside_a_repository_both_git_entries_say_so() {
        let facts = FileContextFacts {
            in_git_repo: false,
            has_github_remote: false,
            ..all_true()
        };
        let items = items(&facts);
        for action in [FileContextAction::CopyPermalink, FileContextAction::ViewFileHistory] {
            let item = items.iter().find(|item| item.action == action).unwrap();
            assert_eq!(
                item.disabled_reason.as_deref(),
                Some("This file is not in a git repository")
            );
        }
    }

    #[test]
    fn groups_never_decrease_so_separators_can_be_drawn_from_them() {
        let items = items(&all_true());
        for pair in items.windows(2) {
            assert!(
                pair[1].group >= pair[0].group,
                "{} must not come after a higher group",
                pair[1].label
            );
        }
    }

    #[test]
    fn the_workspace_owns_every_entry_it_has_to_reach_outside_for() {
        let items = items(&all_true());
        for item in &items {
            let expected = match item.action {
                FileContextAction::Cut
                | FileContextAction::Copy
                | FileContextAction::CopyAndTrim
                | FileContextAction::Paste
                | FileContextAction::OpenMarkdownPreview => FileContextRoute::View,
                _ => FileContextRoute::App,
            };
            assert_eq!(item.route, expected, "wrong route for {}", item.label);
        }
    }
}


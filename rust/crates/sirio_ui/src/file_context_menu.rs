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

/// Drops the indentation every non-blank line shares, so a snippet copied out
/// of a nested block pastes flush left.
pub fn trim_common_indent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(leading_whitespace)
        .reduce(common_prefix)
        .unwrap_or("");
    if indent.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line.strip_prefix(indent).unwrap_or_else(|| line.trim_start()));
    }
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Indentation is ASCII whitespace, so byte slicing is character-safe here.
fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

fn common_prefix<'a>(left: &'a str, right: &'a str) -> &'a str {
    let shared = left
        .bytes()
        .zip(right.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    &left[..shared]
}

/// What "Add to Agent Thread" hands over: where the code is, then the code.
/// The location leads so the agent can act on the file rather than on a
/// floating quotation.
pub fn agent_payload(path: &str, lines: (usize, usize), text: &str, language: &str) -> String {
    let location = if lines.0 == lines.1 {
        format!("{path}:{}", lines.0)
    } else {
        format!("{path}:{}-{}", lines.0, lines.1)
    };
    let fence = "`".repeat(longest_backtick_run(text).max(2) + 1);
    let body = text.trim_end_matches('\n');
    format!("{location}\n\n{fence}{language}\n{body}\n{fence}\n")
}

fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for byte in text.bytes() {
        if byte == b'`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// A GitHub blob URL pinned to a commit, so it keeps resolving after the
/// branch moves.
pub fn permalink(
    owner: &str,
    project: &str,
    sha: &str,
    repo_relative_path: &str,
    lines: (usize, usize),
) -> String {
    let path = repo_relative_path
        .split('/')
        .map(escape_segment)
        .collect::<Vec<_>>()
        .join("/");
    let anchor = if lines.0 == lines.1 {
        format!("#L{}", lines.0)
    } else {
        format!("#L{}-L{}", lines.0, lines.1)
    };
    format!("https://github.com/{owner}/{project}/blob/{sha}/{path}{anchor}")
}

/// Only the characters that would break the URL itself: a space ends it, a
/// `#` starts the fragment, a `?` starts the query.
fn escape_segment(segment: &str) -> String {
    segment
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
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
#[test]
fn trimming_removes_the_indentation_every_line_shares() {
    let text = "    let a = 1;\n    let b = 2;\n";
    assert_eq!(trim_common_indent(text), "let a = 1;\nlet b = 2;\n");
}

#[test]
fn trimming_keeps_relative_indentation() {
    let text = "    if x {\n        y();\n    }\n";
    assert_eq!(trim_common_indent(text), "if x {\n    y();\n}\n");
}

#[test]
fn a_line_at_column_zero_means_nothing_is_trimmed() {
    let text = "fn main() {\n    body();\n}\n";
    assert_eq!(trim_common_indent(text), text);
}

#[test]
fn blank_lines_do_not_defeat_trimming_and_stay_blank() {
    let text = "    a\n\n    b\n";
    assert_eq!(trim_common_indent(text), "a\n\nb\n");
}

#[test]
fn tabs_are_trimmed_as_the_characters_they_are() {
    let text = "\t\tone\n\t\ttwo\n";
    assert_eq!(trim_common_indent(text), "one\ntwo\n");
}

#[test]
fn the_agent_gets_a_located_reference_not_orphan_text() {
    let payload = agent_payload("src/lib.rs", (42, 58), "let a = 1;", "rust");
    assert_eq!(payload, "src/lib.rs:42-58\n\n```rust\nlet a = 1;\n```\n");
}

#[test]
fn a_one_line_reference_names_a_single_line() {
    let payload = agent_payload("src/lib.rs", (42, 42), "let a = 1;", "rust");
    assert!(payload.starts_with("src/lib.rs:42\n"), "got {payload:?}");
}

#[test]
fn a_selection_containing_a_fence_is_wrapped_in_a_longer_one() {
    let payload = agent_payload("notes.md", (1, 3), "```\ncode\n```", "markdown");
    assert!(payload.contains("````markdown\n"), "got {payload:?}");
    assert!(payload.trim_end().ends_with("````"), "got {payload:?}");
}

#[test]
fn a_permalink_points_at_a_commit_and_a_line_range() {
    assert_eq!(
        permalink("ai-sirio", "sirio", "abc123", "rust/crates/sirio_ui/src/lib.rs", (10, 12)),
        "https://github.com/ai-sirio/sirio/blob/abc123/rust/crates/sirio_ui/src/lib.rs#L10-L12"
    );
}

#[test]
fn a_single_line_permalink_has_one_anchor() {
    assert!(
        permalink("o", "p", "sha", "a.rs", (7, 7)).ends_with("#L7"),
        "a one-line selection must not produce a range anchor"
    );
}

#[test]
fn a_path_with_a_space_is_escaped() {
    assert!(
        permalink("o", "p", "sha", "my notes/a b.md", (1, 1))
            .contains("/my%20notes/a%20b.md#L1"),
        "spaces must be percent-encoded"
    );
}
}

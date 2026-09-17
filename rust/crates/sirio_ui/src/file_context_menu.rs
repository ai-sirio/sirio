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
const NO_DEFINITION: &str = "No language server offers definitions here";
const NO_REFERENCES: &str = "No language server offers references here";

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
    GoToDefinition,
    FindReferences,
    /// Never drawn — absent from `ITEMS` — only dispatched to: a `Redirected`
    /// row's click lands here instead of on the action it is labelled with.
    InstallLanguageServer,
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
    /// What the row is beyond its label. The render site shows the note for
    /// both `Unavailable` and `Redirected`, and attaches a click handler for
    /// everything but `Unavailable` — dispatching `to` rather than `action`
    /// for a `Redirected` row.
    pub state: ItemState,
    /// A separator is drawn wherever this changes between adjacent entries.
    pub group: u8,
}

/// What a row is, beyond its label. `disabled_reason: Option<String>`
/// carried two facts in one — *why*, and *clickable or not* — and could not
/// express a row that shows its reason and is still clickable, on a
/// different action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemState {
    Ready,
    /// Present with its reason, no click handler. The original contract,
    /// unchanged where it was right.
    Unavailable(String),
    /// Present with a note, and the click does something *else*: installing
    /// what is missing rather than the action it is labelled with.
    Redirected {
        note: String,
        to: FileContextAction,
    },
}

/// What the workspace found out about this file's language server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MissingServer {
    /// Sirio can fetch it: the command to name, and the download size for
    /// the note. `bytes` is 0 when no size is known before the click — an
    /// npm package — and the note then omits it rather than inventing one.
    Installable { command: String, bytes: u64 },
    /// Something has to come first: the command to name, and what it needs.
    Manual { command: String, needs: String },
    /// An entry of the reader's own, which carries no recipe on purpose —
    /// Sirio must not offer a second copy of a server they already chose.
    /// The name they wrote is all there is to give back.
    NotInstalled { command: String },
}

/// Everything the table needs to decide what to hide and what to disable.
///
/// Not `Copy`, for one field. `missing_language_server` carries owned strings
/// read from the workspace's answer, and a name that cannot be shown is the
/// whole failure this field exists to end.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileContextFacts {
    pub has_selection: bool,
    pub is_markdown: bool,
    pub in_git_repo: bool,
    pub has_github_remote: bool,
    pub has_agent_chat: bool,
    pub definition_available: bool,
    pub references_available: bool,
    /// What the workspace found out about this file's language server.
    /// `Some` only when a launch was attempted and found nothing to launch:
    /// a server that is running, or that ran and broke, leaves it `None`.
    pub missing_language_server: Option<MissingServer>,
}

const ITEMS: [FileContextItem; 12] = [
    FileContextItem {
        label: "Add to Agent Thread",
        action: FileContextAction::SendToAgent,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 0,
    },
    FileContextItem {
        label: "Cut",
        action: FileContextAction::Cut,
        route: FileContextRoute::View,
        state: ItemState::Ready,
        group: 1,
    },
    FileContextItem {
        label: "Copy",
        action: FileContextAction::Copy,
        route: FileContextRoute::View,
        state: ItemState::Ready,
        group: 1,
    },
    FileContextItem {
        label: "Copy and Trim",
        action: FileContextAction::CopyAndTrim,
        route: FileContextRoute::View,
        state: ItemState::Ready,
        group: 1,
    },
    FileContextItem {
        label: "Paste",
        action: FileContextAction::Paste,
        route: FileContextRoute::View,
        state: ItemState::Ready,
        group: 1,
    },
    FileContextItem {
        label: "Reveal in File Manager",
        action: FileContextAction::RevealInFileManager,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 2,
    },
    FileContextItem {
        label: "Open in Terminal",
        action: FileContextAction::OpenInTerminal,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 2,
    },
    FileContextItem {
        label: "Open Markdown Preview",
        action: FileContextAction::OpenMarkdownPreview,
        route: FileContextRoute::View,
        state: ItemState::Ready,
        group: 2,
    },
    FileContextItem {
        label: "Copy Permalink to Line",
        action: FileContextAction::CopyPermalink,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 3,
    },
    FileContextItem {
        label: "View File History",
        action: FileContextAction::ViewFileHistory,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 3,
    },
    FileContextItem {
        label: "Go to Definition",
        action: FileContextAction::GoToDefinition,
        route: FileContextRoute::App,
        state: ItemState::Ready,
        group: 3,
    },
    FileContextItem {
        label: "Find References",
        action: FileContextAction::FindReferences,
        route: FileContextRoute::App,
        state: ItemState::Ready,
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
            item.state = state_for(item.action, facts);
            item
        })
        .collect()
}

fn state_for(action: FileContextAction, facts: &FileContextFacts) -> ItemState {
    let unavailable = |reason: &str| ItemState::Unavailable(reason.to_owned());
    match action {
        FileContextAction::SendToAgent => {
            if !facts.has_selection {
                unavailable(NO_SELECTION)
            } else if !facts.has_agent_chat {
                unavailable(NO_AGENT_CHAT)
            } else {
                ItemState::Ready
            }
        }
        FileContextAction::Cut | FileContextAction::Copy | FileContextAction::CopyAndTrim => {
            if facts.has_selection {
                ItemState::Ready
            } else {
                unavailable(NO_SELECTION)
            }
        }
        FileContextAction::CopyPermalink => {
            if !facts.in_git_repo {
                unavailable(NOT_IN_GIT)
            } else if !facts.has_github_remote {
                unavailable(NO_GITHUB_REMOTE)
            } else {
                ItemState::Ready
            }
        }
        FileContextAction::ViewFileHistory => {
            if facts.in_git_repo {
                ItemState::Ready
            } else {
                unavailable(NOT_IN_GIT)
            }
        }
        FileContextAction::GoToDefinition => {
            if facts.definition_available {
                ItemState::Ready
            } else {
                navigation_state(NO_DEFINITION, facts)
            }
        }
        FileContextAction::FindReferences => {
            if facts.references_available {
                ItemState::Ready
            } else {
                navigation_state(NO_REFERENCES, facts)
            }
        }
        FileContextAction::Paste
        | FileContextAction::RevealInFileManager
        | FileContextAction::OpenInTerminal
        | FileContextAction::OpenMarkdownPreview => ItemState::Ready,
        // Never drawn, so never anything to disable: a stray row would act.
        FileContextAction::InstallLanguageServer => ItemState::Ready,
    }
}

/// What the navigation entries become when no server answers here. A server
/// Sirio can fetch redirects the row at installing it; anything else is
/// merely unavailable — the old contract, unchanged where it was right.
fn navigation_state(fallback: &str, facts: &FileContextFacts) -> ItemState {
    match &facts.missing_language_server {
        Some(MissingServer::Installable { command, bytes }) => ItemState::Redirected {
            note: install_note(command, *bytes),
            to: FileContextAction::InstallLanguageServer,
        },
        Some(MissingServer::Manual { command, needs }) => {
            ItemState::Unavailable(format!("{command} needs {needs}"))
        }
        Some(MissingServer::NotInstalled { command }) => {
            ItemState::Unavailable(format!("{command} is not on PATH"))
        }
        None => ItemState::Unavailable(fallback.to_owned()),
    }
}

/// "clangd is not on PATH — install it (114 MB)". The size rides along
/// because an install button that does not state it is a dishonest one; when
/// no size is known before the click — an npm package — the note omits it
/// rather than inventing one.
///
/// **"not on PATH", not "not installed."** The spawn failed with
/// `NotFound`, which is a fact about `PATH` and not about the machine. A
/// server installed by mason, asdf, mise or a language's own package manager
/// is on disk and invisible here, and "not installed" would send its owner
/// to reinstall something they already have.
fn install_note(command: &str, bytes: u64) -> String {
    if bytes > 0 {
        format!(
            "{command} is not on PATH — install it ({} MB)",
            bytes / 1_000_000
        )
    } else {
        format!("{command} is not on PATH — install it")
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
            definition_available: true,
            references_available: true,
            missing_language_server: None,
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
                "Go to Definition",
                "Find References",
            ]
        );
        assert!(items(&all_true()).iter().all(|item| item.state == ItemState::Ready));
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
            assert_eq!(
                item.state,
                ItemState::Unavailable("Select some text first".to_owned())
            );
        }
        let paste = items.iter().find(|item| item.action == FileContextAction::Paste).unwrap();
        assert_eq!(paste.state, ItemState::Ready, "Paste does not need a selection");
    }

    #[test]
    fn a_missing_chat_disables_only_the_agent_entry() {
        let facts = FileContextFacts { has_agent_chat: false, ..all_true() };
        let items = items(&facts);
        let agent = items.iter().find(|item| item.action == FileContextAction::SendToAgent).unwrap();
        assert_eq!(
            agent.state,
            ItemState::Unavailable("No agent chat open in this worktree".to_owned())
        );
        assert_eq!(
            items.iter().filter(|item| !matches!(item.state, ItemState::Ready)).count(),
            1
        );
    }

    #[test]
    fn a_non_github_remote_disables_the_permalink_but_not_the_history() {
        let facts = FileContextFacts { has_github_remote: false, ..all_true() };
        let items = items(&facts);
        let permalink = items.iter().find(|i| i.action == FileContextAction::CopyPermalink).unwrap();
        assert_eq!(
            permalink.state,
            ItemState::Unavailable("This repository has no GitHub remote".to_owned())
        );
        let history = items.iter().find(|i| i.action == FileContextAction::ViewFileHistory).unwrap();
        assert_eq!(history.state, ItemState::Ready);
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
                item.state,
                ItemState::Unavailable("This file is not in a git repository".to_owned())
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
fn find_references_is_offered_and_explains_itself_when_it_cannot_run() {
// Present-with-a-reason, never absent and never silently inert:
// the reader must learn that no server here answers this.
let facts = FileContextFacts {
references_available: false,
..FileContextFacts::default()
};
let entry = items(&facts)
.into_iter()
.find(|item| item.action == FileContextAction::FindReferences)
.expect("the entry is present even when it cannot run");
assert_eq!(entry.label, "Find References");
assert_eq!(entry.route, FileContextRoute::App);
assert_eq!(entry.state, ItemState::Unavailable(NO_REFERENCES.to_owned()));
}

#[test]
fn find_references_is_enabled_once_a_server_offers_it() {
let facts = FileContextFacts {
references_available: true,
..FileContextFacts::default()
};
let entry = items(&facts)
.into_iter()
.find(|item| item.action == FileContextAction::FindReferences)
.expect("the entry is present");
assert_eq!(entry.state, ItemState::Ready);
}

#[test]
fn find_references_sits_beside_go_to_definition() {
// Same group means no separator between them: they are two halves
// of one question about the symbol under the pointer.
let facts = FileContextFacts::default();
let entries = items(&facts);
let definition = entries
.iter()
.find(|item| item.action == FileContextAction::GoToDefinition)
.expect("go to definition");
let references = entries
.iter()
.find(|item| item.action == FileContextAction::FindReferences)
.expect("find references");
assert_eq!(definition.group, references.group);
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

    #[test]
    fn a_language_server_that_cannot_be_found_is_named_rather_than_described() {
        // The bug this replaces: a Java file on a machine with no `jdtls`
        // read "No language server offers definitions here", which is a
        // sentence about Sirio. The table names a server for every language
        // the editor opens; the only thing missing was on the reader's own
        // machine, and the menu was the one place that could say so.
        let facts = FileContextFacts {
            definition_available: false,
            references_available: false,
            missing_language_server: Some(MissingServer::NotInstalled {
                command: "jdtls".to_owned(),
            }),
            ..FileContextFacts::default()
        };
        let states: Vec<ItemState> = items(&facts)
            .into_iter()
            .filter(|item| {
                matches!(
                    item.action,
                    FileContextAction::GoToDefinition | FileContextAction::FindReferences
                )
            })
            .map(|item| item.state)
            .collect();
        assert_eq!(
            states,
            vec![
                ItemState::Unavailable("jdtls is not on PATH".to_owned()),
                ItemState::Unavailable("jdtls is not on PATH".to_owned()),
            ]
        );
    }

    #[test]
    fn a_server_that_is_there_and_answers_nothing_still_says_so_generically() {
        // The other half: when the program exists, "not installed" would be
        // a lie, and the honest answer is the one about this position.
        let facts = FileContextFacts {
            definition_available: false,
            references_available: false,
            missing_language_server: None,
            ..FileContextFacts::default()
        };
        let entry = items(&facts)
            .into_iter()
            .find(|item| item.action == FileContextAction::GoToDefinition)
            .expect("the entry is present even when it cannot run");
        assert_eq!(
            entry.state,
            ItemState::Unavailable(NO_DEFINITION.to_owned())
        );
    }

    #[test]
    fn a_missing_server_redirects_the_navigation_entries_to_installing_it() {
        let facts = FileContextFacts {
            definition_available: false,
            references_available: false,
            missing_language_server: Some(MissingServer::Installable {
                command: "clangd".into(),
                bytes: 114_790_601,
            }),
            ..FileContextFacts::default()
        };
        let states: Vec<ItemState> = items(&facts)
            .into_iter()
            .filter(|item| {
                matches!(
                    item.action,
                    FileContextAction::GoToDefinition | FileContextAction::FindReferences
                )
            })
            .map(|item| item.state)
            .collect();
        for state in &states {
            match state {
                ItemState::Redirected { note, to } => {
                    assert_eq!(note, "clangd is not on PATH — install it (114 MB)");
                    assert_eq!(*to, FileContextAction::InstallLanguageServer);
                }
                other => panic!("expected a redirect to installing, got {other:?}"),
            }
        }
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn a_server_that_needs_a_toolchain_first_is_merely_unavailable() {
        // Nothing to click: Sirio cannot install a JVM, and a button that
        // pretends otherwise is worse than a greyed row.
        let facts = FileContextFacts {
            missing_language_server: Some(MissingServer::Manual {
                command: "jdtls".into(),
                needs: "a JVM (Java 21 or newer)".into(),
            }),
            ..FileContextFacts::default()
        };
        let entry = items(&facts)
            .into_iter()
            .find(|item| item.action == FileContextAction::GoToDefinition)
            .expect("the entry is present");
        assert_eq!(
            entry.state,
            ItemState::Unavailable("jdtls needs a JVM (Java 21 or newer)".into())
        );
    }

    #[test]
    fn copy_permalink_outside_a_repo_is_still_merely_unavailable() {
        // The old contract survived where it was right, rather than being
        // weakened for everything.
        let facts = FileContextFacts {
            in_git_repo: false,
            ..FileContextFacts::default()
        };
        let entry = items(&facts)
            .into_iter()
            .find(|item| item.action == FileContextAction::CopyPermalink)
            .expect("the entry is present");
        assert_eq!(entry.state, ItemState::Unavailable(NOT_IN_GIT.to_owned()));
    }
}

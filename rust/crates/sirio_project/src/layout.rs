use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The surface kinds a worktree can host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Terminal,
    Chat,
    Document,
    Diff,
    Browser,
}

/// Stable content identity helpers. Document paths are canonicalized before
/// identity is formed, so opening a symlink and its target resolves to one
/// worktree-scoped document id.
pub fn worktree_content_id(worktree_id: &str) -> String {
    format!("worktree:{worktree_id}")
}

pub fn terminal_content_id(worktree_id: &str, terminal_id: &str) -> String {
    format!("terminal:{worktree_id}:{terminal_id}")
}

pub fn browser_content_id(worktree_id: &str, browser_id: &str) -> String {
    format!("browser:{worktree_id}:{browser_id}")
}

pub fn document_content_id(
    worktree_id: &str,
    worktree_root: &Path,
    path: &Path,
) -> std::io::Result<String> {
    let root = std::fs::canonicalize(worktree_root)?;
    let canonical = std::fs::canonicalize(path)?;
    let relative = canonical.strip_prefix(&root).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "document is outside worktree",
        )
    })?;
    Ok(format!(
        "document:{worktree_id}:{}",
        relative.to_string_lossy()
    ))
}

/// The legacy tab content variants still needed by the workspace domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyWorkspaceTab {
    Terminal {
        title: String,
        pane_ids: Vec<String>,
    },
    MarkdownDocument {
        title: String,
        path: PathBuf,
    },
    CodeDocument {
        title: String,
        path: PathBuf,
    },
    Chat {
        title: String,
        tab_id: String,
        agent_id: String,
        session_id: String,
    },
}

impl LegacyWorkspaceTab {
    pub fn title(&self) -> &str {
        match self {
            Self::Terminal { title, .. }
            | Self::MarkdownDocument { title, .. }
            | Self::CodeDocument { title, .. }
            | Self::Chat { title, .. } => title,
        }
    }

    pub fn content_id(&self, worktree_id: &str, worktree_root: &Path) -> Option<String> {
        match self {
            Self::Terminal { pane_ids, .. } => pane_ids
                .first()
                .map(|pane| terminal_content_id(worktree_id, pane)),
            Self::MarkdownDocument { path, .. } | Self::CodeDocument { path, .. } => {
                document_content_id(worktree_id, worktree_root, path).ok()
            }
            Self::Chat { tab_id, .. } => Some(format!("chat:{worktree_id}:{tab_id}")),
        }
    }

    pub fn activity_pane_ids(&self) -> &[String] {
        match self {
            Self::Terminal { pane_ids, .. } => pane_ids,
            _ => &[],
        }
    }

    pub fn activity_tab_id(&self) -> Option<&str> {
        match self {
            Self::Chat { tab_id, .. } => Some(tab_id),
            _ => None,
        }
    }
}

/// A tab's persisted view state. Surface owners may use only the fields that
/// apply to their kind; keeping the state together makes restore lossless.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTabViewState {
    pub editor_caret: Option<usize>,
    pub editor_selection: Option<(usize, usize)>,
    pub editor_scroll: Option<(u32, u32)>,
    pub editor_folds: Vec<usize>,
    pub chat_draft: String,
    pub chat_attachments: Vec<String>,
    pub chat_transcript: String,
    pub chat_follows_tail: bool,
    pub terminal_viewport: Option<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTab {
    pub id: String,
    pub content_id: String,
    pub kind: ContentKind,
    pub title: String,
    pub view_state: WorkspaceTabViewState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneGroup {
    pub id: String,
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutNode {
    Group(String),
    Split {
        id: String,
        axis: SplitAxis,
        fraction_millis: u16,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    fn validate_node(
        &self,
        groups: &mut Vec<String>,
        splits: &mut BTreeSet<String>,
    ) -> Result<(), LayoutError> {
        match self {
            Self::Group(id) => groups.push(id.clone()),
            Self::Split {
                id,
                fraction_millis,
                first,
                second,
                ..
            } => {
                if *fraction_millis > 1_000 {
                    return Err(LayoutError::InvalidFraction(*fraction_millis));
                }
                if !splits.insert(id.clone()) {
                    return Err(LayoutError::DuplicateSplit(id.clone()));
                }
                first.validate_node(groups, splits)?;
                second.validate_node(groups, splits)?;
            }
        }
        Ok(())
    }
}

/// A valid workspace split layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLayout {
    pub root: LayoutNode,
    pub groups: BTreeMap<String, PaneGroup>,
    pub active_group: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    EmptyNonRootGroup(String),
    UnknownGroup(String),
    DuplicateGroup(String),
    DuplicateSplit(String),
    DuplicateTab(String),
    UnknownTab(String),
    InvalidActiveGroup(String),
    InvalidActiveTab(String),
    InvalidFraction(u16),
    GroupNotInTree(String),
    DuplicateContent(String),
}

impl WorkspaceLayout {
    pub fn empty(group_id: impl Into<String>) -> Self {
        let group_id = group_id.into();
        let mut groups = BTreeMap::new();
        groups.insert(
            group_id.clone(),
            PaneGroup {
                id: group_id.clone(),
                tabs: Vec::new(),
                active_tab: None,
            },
        );
        Self {
            root: LayoutNode::Group(group_id.clone()),
            groups,
            active_group: group_id,
        }
    }

    pub fn validate(&self) -> Result<(), LayoutError> {
        let mut tree_groups = Vec::new();
        let mut tree_splits = BTreeSet::new();
        self.root
            .validate_node(&mut tree_groups, &mut tree_splits)?;
        let mut seen_groups = BTreeSet::new();
        for group_id in &tree_groups {
            if !seen_groups.insert(group_id.clone()) {
                return Err(LayoutError::DuplicateGroup(group_id.clone()));
            }
            if !self.groups.contains_key(group_id) {
                return Err(LayoutError::UnknownGroup(group_id.clone()));
            }
        }
        for group_id in self.groups.keys() {
            if !seen_groups.contains(group_id) {
                return Err(LayoutError::GroupNotInTree(group_id.clone()));
            }
        }
        let root_group = tree_groups.first().cloned().unwrap_or_default();
        for group_id in &tree_groups {
            let group = &self.groups[group_id];
            if *group_id != root_group && group.tabs.is_empty() {
                return Err(LayoutError::EmptyNonRootGroup(group_id.clone()));
            }
            if let Some(active) = &group.active_tab
                && !group.tabs.iter().any(|tab| &tab.id == active)
            {
                return Err(LayoutError::InvalidActiveTab(active.clone()));
            }
        }
        if !self.groups.contains_key(&self.active_group) {
            return Err(LayoutError::InvalidActiveGroup(self.active_group.clone()));
        }
        let mut tabs = BTreeSet::new();
        let mut content_ids = BTreeSet::new();
        for group in self.groups.values() {
            for tab in &group.tabs {
                if !tabs.insert(tab.id.clone()) {
                    return Err(LayoutError::DuplicateTab(tab.id.clone()));
                }
                if !content_ids.insert(tab.content_id.clone()) {
                    return Err(LayoutError::DuplicateContent(tab.content_id.clone()));
                }
            }
        }
        Ok(())
    }
}

/// Commands which mutate a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutCommand {
    Insert {
        group: String,
        tab: WorkspaceTab,
    },
    Split {
        group: String,
        new_group: String,
        split_id: String,
        axis: SplitAxis,
    },
    Move {
        tab: String,
        from: String,
        to: String,
    },
    Close {
        tab: String,
        group: String,
    },
    Activate {
        group: String,
        tab: String,
    },
    SetDividerFraction {
        split_id: String,
        fraction_millis: u16,
    },
    UpdateViewState {
        tab: String,
        state: WorkspaceTabViewState,
    },
    Rename {
        tab: String,
        title: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusIntent {
    None,
    Tab,
    Divider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutTransition {
    pub structural: bool,
    pub focus: FocusIntent,
}

pub fn classify_layout_command(command: &LayoutCommand) -> LayoutTransition {
    match command {
        LayoutCommand::Insert { .. }
        | LayoutCommand::Split { .. }
        | LayoutCommand::Move { .. }
        | LayoutCommand::Close { .. } => LayoutTransition {
            structural: true,
            focus: FocusIntent::Tab,
        },
        LayoutCommand::Activate { .. }
        | LayoutCommand::UpdateViewState { .. }
        | LayoutCommand::Rename { .. } => LayoutTransition {
            structural: false,
            focus: FocusIntent::Tab,
        },
        LayoutCommand::SetDividerFraction { .. } => LayoutTransition {
            structural: false,
            focus: FocusIntent::Divider,
        },
    }
}

/// Schema-versioned canonical JSON snapshot envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub schema_version: u32,
    pub layout: WorkspaceLayout,
}

#[derive(Debug)]
pub enum SnapshotError {
    InvalidJson,
    MissingVersion,
    FutureVersion(u32),
}

impl WorkspaceSnapshot {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn encode(layout: &WorkspaceLayout) -> Result<String, SnapshotError> {
        serde_json::to_string(&Self {
            schema_version: Self::CURRENT_VERSION,
            layout: layout.clone(),
        })
        .map_err(|_| SnapshotError::InvalidJson)
    }

    pub fn decode(raw: &str) -> Result<Self, SnapshotError> {
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|_| SnapshotError::InvalidJson)?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(SnapshotError::MissingVersion)? as u32;
        if version > Self::CURRENT_VERSION {
            return Err(SnapshotError::FutureVersion(version));
        }
        serde_json::from_value(value).map_err(|_| SnapshotError::InvalidJson)
    }

    pub fn decode_or_empty(raw: &str, group_id: &str) -> WorkspaceLayout {
        Self::decode(raw)
            .map(|snapshot| snapshot.layout)
            .unwrap_or_else(|_| WorkspaceLayout::empty(group_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn document_identity_is_worktree_scoped_and_resolves_symlinks() {
        let root = std::env::temp_dir().join(format!("sirio-layout-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("note.md");
        let link = root.join("alias.md");
        fs::write(&target, "note").unwrap();
        if let Err(error) = crate::test_symlink::file(&target, &link) {
            // See the sibling in `file.rs`: symlink RESOLUTION is what this
            // asserts, and that is portable; only creating the fixture needs
            // a privilege Windows withholds by default.
            assert!(
                crate::test_symlink::is_unprivileged(&error),
                "symlink fixture failed for an unexpected reason: {error}"
            );
            let _ = fs::remove_dir_all(&root);
            return;
        }
        assert_eq!(
            document_content_id("w1", &root, &target).unwrap(),
            document_content_id("w1", &root, &link).unwrap()
        );
        assert_ne!(
            document_content_id("w1", &root, &target).unwrap(),
            document_content_id("w2", &root, &target).unwrap()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_content_exposes_only_terminal_panes_and_chat_tab_activity() {
        let terminal = LegacyWorkspaceTab::Terminal {
            title: "Terminal".into(),
            pane_ids: vec!["p1".into(), "p2".into()],
        };
        assert_eq!(
            terminal.activity_pane_ids(),
            &["p1".to_string(), "p2".to_string()]
        );
        let chat = LegacyWorkspaceTab::Chat {
            title: "Chat".into(),
            tab_id: "tab".into(),
            agent_id: "codex".into(),
            session_id: "s".into(),
        };
        assert_eq!(chat.activity_tab_id(), Some("tab"));
        assert!(chat.activity_pane_ids().is_empty());
    }

    #[test]
    fn layout_validation_accepts_root_empty_but_rejects_invalid_references() {
        let layout = WorkspaceLayout::empty("root");
        assert!(layout.validate().is_ok());
        let mut invalid = layout.clone();
        invalid.active_group = "missing".into();
        assert_eq!(
            invalid.validate(),
            Err(LayoutError::InvalidActiveGroup("missing".into()))
        );

        let mut split = WorkspaceLayout::empty("root");
        split.root = LayoutNode::Split {
            id: "split".into(),
            axis: SplitAxis::Horizontal,
            fraction_millis: 1_001,
            first: Box::new(LayoutNode::Group("root".into())),
            second: Box::new(LayoutNode::Group("missing".into())),
        };
        assert_eq!(split.validate(), Err(LayoutError::InvalidFraction(1_001)));
    }

    #[test]
    fn command_classes_distinguish_structural_and_nonstructural_changes() {
        let tab = WorkspaceTab {
            id: "t".into(),
            content_id: "c".into(),
            kind: ContentKind::Terminal,
            title: "T".into(),
            view_state: WorkspaceTabViewState::default(),
        };
        assert!(
            classify_layout_command(&LayoutCommand::Insert {
                group: "g".into(),
                tab
            })
            .structural
        );
        assert_eq!(
            classify_layout_command(&LayoutCommand::SetDividerFraction {
                split_id: "s".into(),
                fraction_millis: 500
            })
            .focus,
            FocusIntent::Divider
        );
        assert!(
            !classify_layout_command(&LayoutCommand::Rename {
                tab: "t".into(),
                title: "new".into()
            })
            .structural
        );
    }

    #[test]
    fn snapshots_are_versioned_canonical_and_malformed_data_falls_back_empty() {
        let layout = WorkspaceLayout::empty("root");
        let encoded = WorkspaceSnapshot::encode(&layout).unwrap();
        assert!(encoded.contains("\"schema_version\":1"));
        assert!(!encoded.contains("\\/"));
        assert_eq!(WorkspaceSnapshot::decode(&encoded).unwrap().layout, layout);
        assert!(matches!(
            WorkspaceSnapshot::decode("{}"),
            Err(SnapshotError::MissingVersion)
        ));
        assert!(matches!(
            WorkspaceSnapshot::decode(r#"{"schema_version":2}"#),
            Err(SnapshotError::FutureVersion(2))
        ));
        assert_eq!(
            WorkspaceSnapshot::decode_or_empty("not json", "fallback"),
            WorkspaceLayout::empty("fallback")
        );
    }
}

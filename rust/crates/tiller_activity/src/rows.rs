//! Activity-panel projection of open worktree content.

use std::collections::HashMap;

use crate::activity::ActivityStatus;
use crate::status::AgentStatus;

/// The content kinds that can contribute an activity source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivityTabKind {
    /// Each live pane id produces one terminal activity row.
    Terminal {
        pane_ids: Vec<String>,
    },
    /// Chat activity is keyed by the tab id itself.
    Chat,
    Document,
    Diff,
    Browser,
}

/// Minimal tab input needed to build activity rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityTab {
    pub id: String,
    pub title: String,
    pub kind: ActivityTabKind,
}

impl ActivityTab {
    pub fn new(id: impl Into<String>, title: impl Into<String>, kind: ActivityTabKind) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
        }
    }

    pub fn terminal<I, S>(id: impl Into<String>, title: impl Into<String>, pane_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(
            id,
            title,
            ActivityTabKind::Terminal {
                pane_ids: pane_ids.into_iter().map(Into::into).collect(),
            },
        )
    }

    pub fn chat(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self::new(id, title, ActivityTabKind::Chat)
    }
}

/// Open tabs belonging to one worktree, in their presentation order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityWorktreeInput {
    pub worktree_id: String,
    pub label: String,
    pub tabs: Vec<ActivityTab>,
}

impl ActivityWorktreeInput {
    pub fn new(
        worktree_id: impl Into<String>,
        label: impl Into<String>,
        tabs: Vec<ActivityTab>,
    ) -> Self {
        Self {
            worktree_id: worktree_id.into(),
            label: label.into(),
            tabs,
        }
    }
}

/// Whether an activity row comes from a terminal leaf or a chat tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityRowKind {
    Terminal,
    Chat,
}

/// One flattened activity source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub id: String,
    pub worktree_id: String,
    pub tab_id: String,
    pub activity_id: String,
    pub kind: ActivityRowKind,
    pub title: String,
    pub agent_id: Option<String>,
    pub worktree_label: String,
    pub status: ActivityStatus,
}

/// Flattens open worktree tabs while retaining unrecognized live terminals as
/// idle rows and omitting document, diff, and browser content.
pub fn build_activity_rows(
    worktrees: &[ActivityWorktreeInput],
    agent_status: &HashMap<String, AgentStatus>,
    pane_agents: &HashMap<String, String>,
) -> Vec<ActivityRow> {
    let mut rows = Vec::new();
    for worktree in worktrees {
        for tab in &worktree.tabs {
            match &tab.kind {
                ActivityTabKind::Terminal { pane_ids } => {
                    for pane_id in pane_ids {
                        rows.push(make_row(
                            worktree,
                            tab,
                            pane_id,
                            ActivityRowKind::Terminal,
                            agent_status,
                            pane_agents,
                        ));
                    }
                }
                ActivityTabKind::Chat => rows.push(make_row(
                    worktree,
                    tab,
                    &tab.id,
                    ActivityRowKind::Chat,
                    agent_status,
                    pane_agents,
                )),
                ActivityTabKind::Document | ActivityTabKind::Diff | ActivityTabKind::Browser => {}
            }
        }
    }
    rows
}

fn make_row(
    worktree: &ActivityWorktreeInput,
    tab: &ActivityTab,
    activity_id: &str,
    kind: ActivityRowKind,
    agent_status: &HashMap<String, AgentStatus>,
    pane_agents: &HashMap<String, String>,
) -> ActivityRow {
    ActivityRow {
        id: format!("{}:{}:{}", worktree.worktree_id, tab.id, activity_id),
        worktree_id: worktree.worktree_id.clone(),
        tab_id: tab.id.clone(),
        activity_id: activity_id.to_string(),
        kind,
        title: tab.title.clone(),
        agent_id: pane_agents.get(activity_id).cloned(),
        worktree_label: worktree.label.clone(),
        status: ActivityStatus::from_agent_status(agent_status.get(activity_id).copied()),
    }
}

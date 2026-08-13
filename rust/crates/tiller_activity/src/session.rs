//! Stable agent-session references and safe restore planning.

use std::collections::HashSet;

/// Stable identity of a terminal content surface across relaunches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TerminalContentId(String);

impl TerminalContentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Native session identity reported by an agent. It is keyed by stable
/// terminal content, never by the live pane generation that gets reminted on
/// every mount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSessionRef {
    pub content_id: TerminalContentId,
    pub agent_id: String,
    pub session_ref: String,
}

impl AgentSessionRef {
    pub fn new(
        content_id: TerminalContentId,
        agent_id: impl Into<String>,
        session_ref: impl Into<String>,
    ) -> Self {
        Self {
            content_id,
            agent_id: agent_id.into(),
            session_ref: session_ref.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSessionRestoreResult {
    pub resumable: Vec<AgentSessionRef>,
    pub prunable: Vec<AgentSessionRef>,
}

/// Pure planner for splitting saved refs before restore mutates persistence.
pub struct AgentSessionRestorePlan;

impl AgentSessionRestorePlan {
    /// Keeps refs whose stable content id is present in the restored layout;
    /// everything else is safe to prune. Callers must pass a loaded layout,
    /// not a pre-restore empty set.
    pub fn plan(
        refs: &[AgentSessionRef],
        live_content_ids: &HashSet<TerminalContentId>,
    ) -> AgentSessionRestoreResult {
        let mut resumable = Vec::new();
        let mut prunable = Vec::new();
        for reference in refs {
            if live_content_ids.contains(&reference.content_id) {
                resumable.push(reference.clone());
            } else {
                prunable.push(reference.clone());
            }
        }
        AgentSessionRestoreResult {
            resumable,
            prunable,
        }
    }
}

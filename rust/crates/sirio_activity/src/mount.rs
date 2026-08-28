//! Safe eviction policy for mounted worktrees.

use crate::status::AgentStatus;

/// Pure mount-cap policy. The caller owns the actual unmount side effect.
pub struct WorktreeMountPolicy;

impl WorktreeMountPolicy {
    pub fn ids_to_evict<I, Status, Unsaved>(
        open_worktree_ids: &[I],
        selected_worktree_id: Option<&I>,
        cap: usize,
        status: Status,
        has_unsaved_work: Unsaved,
    ) -> Vec<I>
    where
        I: Clone + PartialEq,
        Status: Fn(&I) -> Option<AgentStatus>,
        Unsaved: Fn(&I) -> bool,
    {
        if cap == 0 || open_worktree_ids.len() <= cap {
            return Vec::new();
        }

        let mut mounted_count = open_worktree_ids.len();
        let mut evicted = Vec::new();
        for id in open_worktree_ids {
            if mounted_count <= cap {
                break;
            }
            if selected_worktree_id.is_some_and(|selected| selected == id) {
                continue;
            }
            if matches!(
                status(id),
                Some(AgentStatus::Running | AgentStatus::NeedsInput)
            ) {
                continue;
            }
            if has_unsaved_work(id) {
                continue;
            }
            evicted.push(id.clone());
            mounted_count -= 1;
        }
        evicted
    }
}

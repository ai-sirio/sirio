//! Launch restore ordering.

use std::collections::HashSet;
use std::hash::Hash;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapRestoreResult<T> {
    pub priority: Vec<T>,
    pub deferred: Vec<T>,
}

/// Pure ordering policy for first-paint and deferred worktree restoration.
pub struct BootstrapRestoreOrder;

impl BootstrapRestoreOrder {
    pub fn partition<T, I, F>(
        worktrees: &[T],
        open_worktree_ids: &[I],
        selected_worktree_id: Option<&I>,
        id_of: F,
    ) -> BootstrapRestoreResult<T>
    where
        T: Clone,
        I: Clone + Eq + Hash,
        F: Fn(&T) -> I,
    {
        let mut ordered_ids = Vec::new();
        let mut seen = HashSet::new();

        if let Some(selected) = selected_worktree_id
            && open_worktree_ids.iter().any(|id| id == selected)
        {
            ordered_ids.push(selected.clone());
            seen.insert(selected.clone());
        }
        for id in open_worktree_ids {
            if seen.insert(id.clone()) {
                ordered_ids.push(id.clone());
            }
        }

        let mut priority = Vec::new();
        let mut priority_ids = HashSet::new();
        for id in ordered_ids {
            if let Some(worktree) = worktrees.iter().find(|worktree| id_of(worktree) == id) {
                priority_ids.insert(id);
                priority.push(worktree.clone());
            }
        }
        let deferred = worktrees
            .iter()
            .filter(|worktree| !priority_ids.contains(&id_of(worktree)))
            .cloned()
            .collect();

        BootstrapRestoreResult { priority, deferred }
    }
}

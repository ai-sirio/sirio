//! Shared drag/reorder contract for sidebar and tab rows.

/// The row collection a drag is allowed to mutate. Keeping this in the
/// payload makes cross-surface drops reject themselves before a host model is
/// touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderScope {
    Projects,
    Worktrees,
    Tabs,
}

/// Stable identity carried by a row drag. `group` prevents a worktree or tab
/// from being reordered across an unrelated parent collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowDrag {
    pub scope: ReorderScope,
    pub id: usize,
    pub group: Option<usize>,
}

/// A target is valid only inside the same typed collection and parent group.
pub fn accepts_drop(drag: RowDrag, scope: ReorderScope, group: Option<usize>) -> bool {
    drag.scope == scope && drag.group == group
}

/// Computes the insertion point after removing `from`. `target` is the row
/// currently under the pointer; `before` is derived from the pointer's half
/// of that row. Returning `None` means no visible order change is needed.
pub fn insertion_index(len: usize, from: usize, target: usize, before: bool) -> Option<usize> {
    if from >= len || target >= len || from == target {
        return None;
    }
    let target_after_remove = if from < target { target - 1 } else { target };
    let mut index = target_after_remove + usize::from(!before);
    if index > len - 1 {
        index = len - 1;
    }
    (index != from).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cross_scope_and_cross_group_drops() {
        let drag = RowDrag {
            scope: ReorderScope::Tabs,
            id: 1,
            group: Some(7),
        };
        assert!(accepts_drop(drag, ReorderScope::Tabs, Some(7)));
        assert!(!accepts_drop(drag, ReorderScope::Projects, Some(7)));
        assert!(!accepts_drop(drag, ReorderScope::Tabs, Some(8)));
    }

    #[test]
    fn insertion_point_tracks_live_before_and_after_halves() {
        assert_eq!(insertion_index(4, 0, 2, true), Some(1));
        assert_eq!(insertion_index(4, 0, 2, false), Some(2));
        assert_eq!(insertion_index(4, 3, 1, true), Some(1));
        assert_eq!(insertion_index(4, 3, 1, false), Some(2));
    }
}

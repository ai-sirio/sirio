//! Which terminal pane a connecting Claude runs in (spec 2026-11-15, section 4.2 and 10.3).
//!
//! Pure: the walk takes its parent lookup as a closure, so the caller supplies the platform's
//! (`/proc`, libproc, a toolhelp snapshot) and runs it off the UI thread.

/// How many parent links the walk follows from the connecting pid.
pub const WALK_DEPTH: usize = 32;

/// The pane whose shell is the nearest ancestor of `target` (or `target` itself), walking up
/// through at most [`WALK_DEPTH`] parents. `panes` pairs each pane's key with its shell's pid;
/// a shell pid of 0 means unknown and never matches. A parent that cannot be read, a process
/// with no parent, and a table that loops all end the walk with no owner.
pub fn resolve_owner<K: Clone>(
    target: u32,
    panes: &[(K, u32)],
    mut parent: impl FnMut(u32) -> Option<u32>,
) -> Option<K> {
    let mut current = target;
    for _ in 0..=WALK_DEPTH {
        if current == 0 {
            return None;
        }
        if let Some((key, _)) = panes.iter().find(|(_, shell)| *shell == current) {
            return Some(key.clone());
        }
        current = parent(current)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn table(links: &[(u32, u32)]) -> impl FnMut(u32) -> Option<u32> {
        let map: HashMap<u32, u32> = links.iter().copied().collect();
        move |pid| map.get(&pid).copied()
    }

    #[test]
    fn grandchild_resolves_to_its_pane() {
        // claude (300) <- node (200) <- shell (100)
        let panes = [("a", 100), ("b", 900)];
        let owner = resolve_owner(300, &panes, table(&[(300, 200), (200, 100), (100, 1)]));
        assert_eq!(owner, Some("a"));
    }

    #[test]
    fn unrelated_process_is_none() {
        let panes = [("a", 100)];
        let owner = resolve_owner(300, &panes, table(&[(300, 200), (200, 50), (50, 1)]));
        assert_eq!(owner, None);
    }

    #[test]
    fn parent_cycle_terminates() {
        let panes = [("a", 100)];
        let mut lookups = 0;
        let owner = resolve_owner(300, &panes, |pid| {
            lookups += 1;
            Some(if pid == 300 { 200 } else { 300 })
        });
        assert_eq!(owner, None);
        assert!(lookups <= WALK_DEPTH + 1);
    }

    #[test]
    fn depth_bound_is_respected() {
        let panes = [("a", 1000)];
        // `steps` links between the target and the shell: pid 1000 + steps is the target.
        let chain = |steps: u32| {
            let links: Vec<(u32, u32)> = (0..steps).map(|i| (1000 + steps - i, 999 + steps - i)).collect();
            (1000 + steps, links)
        };
        let (target, links) = chain(32);
        assert_eq!(resolve_owner(target, &panes, table(&links)), Some("a"));
        let (target, links) = chain(33);
        assert_eq!(resolve_owner(target, &panes, table(&links)), None);
    }

    #[test]
    fn nearest_shell_wins() {
        // Pane B's shell runs inside pane A's tree: claude (300) <- B's shell (200) <- A's shell (100).
        let panes = [("a", 100), ("b", 200)];
        let owner = resolve_owner(300, &panes, table(&[(300, 200), (200, 100)]));
        assert_eq!(owner, Some("b"));
    }

    #[test]
    fn missing_parent_ends_the_walk() {
        let panes = [("a", 100)];
        // 200's parent cannot be read: the shell above it is never reached.
        let owner = resolve_owner(300, &panes, table(&[(300, 200)]));
        assert_eq!(owner, None);
    }
}

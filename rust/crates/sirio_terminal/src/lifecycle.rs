use std::collections::{HashMap, HashSet};

/// The stable identity of a terminal surface host.
///
/// A host belongs to terminal content, not to a particular position in the
/// split tree. Relaunching the same content therefore keeps its identity but
/// gets a new generation, which prevents a late event from an old PTY being
/// applied to a fresh one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSurfaceHost {
    content_id: String,
    generation: u64,
    mounted: bool,
}

impl TerminalSurfaceHost {
    pub fn new(content_id: impl Into<String>) -> Self {
        Self {
            content_id: content_id.into(),
            generation: 1,
            mounted: true,
        }
    }

    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_mounted(&self) -> bool {
        self.mounted
    }

    /// Starts a new PTY generation without changing the terminal content ID.
    pub fn relaunch(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.mounted = true;
        self.generation
    }

    /// Marks the host unavailable until the owning pane mounts it again.
    pub fn teardown(&mut self) {
        self.mounted = false;
    }
}

/// A cached controller stays attached to terminal content while its pane is
/// moved within the same worktree. `T` is deliberately generic: the app can
/// put its GPUI entity, PTY handle, or a small controller object here without
/// making this cache depend on the pane-composition crate.
#[derive(Debug)]
pub struct CachedTerminalPane<T> {
    pub worktree_id: String,
    pub pane_id: String,
    pub content_id: String,
    pub controller: T,
}

#[derive(Debug)]
pub struct TerminalPaneCache<T> {
    panes: HashMap<(String, String), CachedTerminalPane<T>>,
    focused_by_worktree: HashMap<String, String>,
}

impl<T> Default for TerminalPaneCache<T> {
    fn default() -> Self {
        Self {
            panes: HashMap::new(),
            focused_by_worktree: HashMap::new(),
        }
    }
}

impl<T> TerminalPaneCache<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts by worktree and stable terminal content ID, returning a
    /// replaced controller only when that exact pane is already cached.
    pub fn insert(
        &mut self,
        worktree_id: impl Into<String>,
        pane_id: impl Into<String>,
        content_id: impl Into<String>,
        controller: T,
    ) -> Option<T> {
        let worktree_id = worktree_id.into();
        let content_id = content_id.into();
        self.panes
            .insert(
                (worktree_id.clone(), content_id.clone()),
                CachedTerminalPane {
                    worktree_id,
                    pane_id: pane_id.into(),
                    content_id,
                    controller,
                },
            )
            .map(|pane| pane.controller)
    }

    pub fn get(&self, content_id: &str) -> Option<&CachedTerminalPane<T>> {
        self.panes
            .values()
            .find(|pane| pane.content_id == content_id)
    }

    pub fn get_mut(&mut self, content_id: &str) -> Option<&mut CachedTerminalPane<T>> {
        self.panes
            .values_mut()
            .find(|pane| pane.content_id == content_id)
    }

    pub fn get_in_worktree(
        &self,
        worktree_id: &str,
        content_id: &str,
    ) -> Option<&CachedTerminalPane<T>> {
        self.panes
            .get(&(worktree_id.to_owned(), content_id.to_owned()))
    }

    pub fn get_mut_in_worktree(
        &mut self,
        worktree_id: &str,
        content_id: &str,
    ) -> Option<&mut CachedTerminalPane<T>> {
        self.panes
            .get_mut(&(worktree_id.to_owned(), content_id.to_owned()))
    }

    pub fn iter_in_worktree(
        &self,
        worktree_id: &str,
    ) -> impl Iterator<Item = &CachedTerminalPane<T>> {
        self.panes
            .values()
            .filter(move |pane| pane.worktree_id == worktree_id)
    }

    pub fn remove(&mut self, content_id: &str) -> Option<CachedTerminalPane<T>> {
        let key = self
            .panes
            .iter()
            .find_map(|(key, pane)| (pane.content_id == content_id).then(|| key.clone()))?;
        self.remove_by_key(&key)
    }

    pub fn remove_in_worktree(
        &mut self,
        worktree_id: &str,
        content_id: &str,
    ) -> Option<CachedTerminalPane<T>> {
        self.remove_by_key(&(worktree_id.to_owned(), content_id.to_owned()))
    }

    pub fn remove_worktree(&mut self, worktree_id: &str) {
        self.panes
            .retain(|_, pane| pane.worktree_id != worktree_id);
        self.focused_by_worktree.remove(worktree_id);
    }

    /// Drops cached controllers that were not attached to the restored tab
    /// tree. This is the cleanup boundary for a legitimate re-creation: a
    /// stale `TerminalView` leaves the cache and its `Drop` closes the PTY.
    pub fn remove_unkept_in_worktree(
        &mut self,
        worktree_id: &str,
        keep_content_ids: &HashSet<String>,
    ) {
        self.panes.retain(|(cached_worktree, content_id), _| {
            cached_worktree != worktree_id || keep_content_ids.contains(content_id)
        });
        if self
            .focused_by_worktree
            .get(worktree_id)
            .is_some_and(|focused| !keep_content_ids.contains(focused))
        {
            self.focused_by_worktree.remove(worktree_id);
        }
    }

    pub fn has_in_worktree(&self, worktree_id: &str) -> bool {
        self.panes
            .keys()
            .any(|(cached_worktree, _)| cached_worktree == worktree_id)
    }

    fn remove_by_key(
        &mut self,
        key: &(String, String),
    ) -> Option<CachedTerminalPane<T>> {
        let removed = self.panes.remove(key);
        if let Some(pane) = removed.as_ref()
            && self
                .focused_by_worktree
                .get(&pane.worktree_id)
                .is_some_and(|focused| focused == &pane.content_id)
        {
            self.focused_by_worktree.remove(&pane.worktree_id);
        }
        removed
    }

    /// Rebinds the visual pane ID while retaining the controller and PTY.
    pub fn move_within_worktree(
        &mut self,
        content_id: &str,
        worktree_id: &str,
        new_pane_id: impl Into<String>,
    ) -> bool {
        let Some(pane) = self.get_mut_in_worktree(worktree_id, content_id) else {
            return false;
        };
        pane.pane_id = new_pane_id.into();
        true
    }

    pub fn focus(&mut self, worktree_id: &str, content_id: &str) -> bool {
        if self.get_in_worktree(worktree_id, content_id).is_some() {
            self.focused_by_worktree
                .insert(worktree_id.to_owned(), content_id.to_owned());
            true
        } else {
            false
        }
    }

    pub fn focused_content_id(&self, worktree_id: &str) -> Option<&str> {
        self.focused_by_worktree
            .get(worktree_id)
            .map(String::as_str)
    }

    pub fn restore_focus(&self, worktree_id: &str) -> Option<&CachedTerminalPane<T>> {
        self.focused_content_id(worktree_id)
            .and_then(|content_id| self.get_in_worktree(worktree_id, content_id))
    }

    pub fn len(&self) -> usize {
        self.panes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_generation_changes_only_on_relaunch_and_teardown_is_explicit() {
        let mut host = TerminalSurfaceHost::new("terminal-1");
        assert_eq!(host.content_id(), "terminal-1");
        assert_eq!(host.generation(), 1);
        assert!(host.is_mounted());

        host.teardown();
        assert!(!host.is_mounted());
        assert_eq!(host.relaunch(), 2);
        assert!(host.is_mounted());
    }

    #[test]
    fn cache_preserves_controller_when_a_pane_moves_and_restores_focus() {
        let mut cache = TerminalPaneCache::new();
        cache.insert("worktree", "pane-a", "terminal-a", "live-pty");
        assert!(cache.focus("worktree", "terminal-a"));
        assert!(cache.move_within_worktree("terminal-a", "worktree", "pane-b"));

        let pane = cache.restore_focus("worktree").expect("focused pane");
        assert_eq!(pane.pane_id, "pane-b");
        assert_eq!(pane.controller, "live-pty");
    }

    #[test]
    fn cache_rejects_a_move_across_worktrees() {
        let mut cache = TerminalPaneCache::new();
        cache.insert("worktree-a", "pane-a", "terminal-a", ());
        assert!(!cache.move_within_worktree("terminal-a", "worktree-b", "pane-b"));
        assert_eq!(cache.get("terminal-a").unwrap().pane_id, "pane-a");
    }

    #[test]
    fn cache_keeps_same_content_ids_isolated_between_worktrees() {
        let mut cache = TerminalPaneCache::new();
        cache.insert("worktree-a", "pane-a", "terminal-0", "pty-a");
        cache.insert("worktree-b", "pane-b", "terminal-0", "pty-b");

        assert_eq!(
            cache
                .get_in_worktree("worktree-a", "terminal-0")
                .unwrap()
                .controller,
            "pty-a"
        );
        assert_eq!(
            cache
                .get_in_worktree("worktree-b", "terminal-0")
                .unwrap()
                .controller,
            "pty-b"
        );
    }

    #[test]
    fn cache_removes_every_controller_for_a_worktree() {
        let mut cache = TerminalPaneCache::new();
        cache.insert("worktree-a", "pane-a", "terminal-a", "pty-a");
        cache.insert("worktree-b", "pane-b", "terminal-b", "pty-b");
        cache.focus("worktree-a", "terminal-a");

        cache.remove_worktree("worktree-a");

        assert!(cache.get_in_worktree("worktree-a", "terminal-a").is_none());
        assert!(cache.restore_focus("worktree-a").is_none());
        assert_eq!(
            cache
                .get_in_worktree("worktree-b", "terminal-b")
                .unwrap()
                .controller,
            "pty-b"
        );
    }

    #[test]
    fn cache_drops_unkept_controllers_after_a_restore() {
        let mut cache = TerminalPaneCache::new();
        cache.insert("worktree-a", "pane-a", "terminal-a", "pty-a");
        cache.insert("worktree-a", "pane-b", "terminal-b", "pty-b");
        cache.focus("worktree-a", "terminal-b");

        cache.remove_unkept_in_worktree(
            "worktree-a",
            &HashSet::from(["terminal-a".to_string()]),
        );

        assert!(cache.get_in_worktree("worktree-a", "terminal-b").is_none());
        assert_eq!(cache.focused_content_id("worktree-a"), None);
        assert_eq!(cache.len(), 1);
    }
}

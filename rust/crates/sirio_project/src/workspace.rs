//! The [`Workspace`]: the whole tree Sirio manages, plus the pure tree
//! operations the sidebar needs.

use std::collections::HashSet;
use std::path::Path;

use crate::{
    GitError, Project, ProjectId, Tab, TabId, TabKind, Worktree, WorktreeId,
    discovery::{self, DiscoveredWorktree},
};

/// The whole tree: projects with their worktrees and tabs, plus the current
/// expansion and selection state.
///
/// A plain value type (no interior mutability, no I/O) so it is trivially
/// testable and can be shared across threads. Collections are flat `Vec`s in
/// insertion order — the sidebar renders in display order, exactly like the
/// Swift app's `AppModel` arrays — and lookups are linear, which is fine at
/// Sirio's scale (a handful of projects, each with a few worktrees).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workspace {
    projects: Vec<Project>,
    worktrees: Vec<Worktree>,
    tabs: Vec<Tab>,
    expanded_projects: HashSet<ProjectId>,
    selected_worktree: Option<WorktreeId>,
    selected_tab: Option<TabId>,
    next_id: u64,
}

/// The result of [`Workspace::filter`]: the rows the sidebar should draw.
///
/// `projects` is ordered like the model. For each project, `is_expanded`
/// tells the caller whether to render the project's worktrees; the
/// worktrees and their tabs are the *visible* subset under the active query
/// (see [`Workspace::filter`] for the exact semantics).
#[derive(Debug, PartialEq, Eq)]
pub struct FilteredTree<'a> {
    /// Projects to draw, in display order.
    pub projects: Vec<FilteredProject<'a>>,
}

/// A project row within a [`FilteredTree`].
#[derive(Debug, PartialEq, Eq)]
pub struct FilteredProject<'a> {
    /// The project itself.
    pub project: &'a Project,
    /// Whether the project is currently expanded. When false the caller
    /// should render only the project row, not its worktrees.
    pub is_expanded: bool,
    /// Worktrees to draw under this project.
    pub worktrees: Vec<FilteredWorktree<'a>>,
}

/// A worktree row within a [`FilteredProject`].
#[derive(Debug, PartialEq, Eq)]
pub struct FilteredWorktree<'a> {
    /// The worktree itself.
    pub worktree: &'a Worktree,
    /// Whether this is the selected worktree.
    pub is_selected: bool,
    /// Tabs to draw under this worktree.
    pub tabs: Vec<FilteredTab<'a>>,
}

/// A tab row within a [`FilteredWorktree`].
#[derive(Debug, PartialEq, Eq)]
pub struct FilteredTab<'a> {
    /// The tab itself.
    pub tab: &'a Tab,
    /// Whether this is the selected tab.
    pub is_selected: bool,
}

impl Workspace {
    /// Creates an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Discovery
    // ------------------------------------------------------------------

    /// Discovers a directory as a project and adds it to the workspace,
    /// together with its worktrees (for a git repo) or nothing (for a
    /// non-git project). Returns the new project's id.
    ///
    /// Branch labels follow the Swift app: the primary checkout falls back
    /// to `main` when HEAD is detached, and a linked detached checkout is
    /// labelled by its short commit id.
    pub fn load_project(&mut self, root: &Path) -> Result<ProjectId, GitError> {
        let discovered = discovery::discover_project(root)?;
        let project_id = ProjectId::new(self.mint_id());
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| root.to_string_lossy().into_owned());

        self.projects.push(Project::new(
            project_id,
            name,
            root.to_path_buf(),
            discovered.is_git,
        ));

        for worktree in discovered.worktrees {
            let worktree = self.worktree_from_discovered(worktree, project_id);
            self.worktrees.push(worktree);
        }

        Ok(project_id)
    }

    fn worktree_from_discovered(
        &mut self,
        discovered: DiscoveredWorktree,
        project_id: ProjectId,
    ) -> Worktree {
        let branch = discovered.branch.clone().unwrap_or_else(|| {
            if discovered.is_primary {
                // Swift parity: the primary checkout's placeholder branch.
                "main".to_string()
            } else {
                // A linked detached checkout is labelled by its commit.
                discovered
                    .head
                    .as_deref()
                    .map(|head| head.chars().take(7).collect::<String>())
                    .unwrap_or_else(|| "HEAD".to_string())
            }
        });

        Worktree::new(
            WorktreeId::new(self.mint_id()),
            project_id,
            branch,
            discovered.path,
            discovered.is_primary,
        )
    }

    // ------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------

    /// All projects, in insertion order.
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// The project with the given id, if any.
    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// The worktree with the given id, if any.
    pub fn worktree(&self, id: WorktreeId) -> Option<&Worktree> {
        self.worktrees.iter().find(|worktree| worktree.id == id)
    }

    /// The tab with the given id, if any.
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    /// The worktrees of a project, in insertion order.
    pub fn worktrees_of(&self, project_id: ProjectId) -> impl Iterator<Item = &Worktree> {
        self.worktrees
            .iter()
            .filter(move |worktree| worktree.project_id == project_id)
    }

    /// The tabs of a worktree, in insertion order.
    pub fn tabs_of(&self, worktree_id: WorktreeId) -> impl Iterator<Item = &Tab> {
        self.tabs
            .iter()
            .filter(move |tab| tab.worktree_id == worktree_id)
    }

    /// The selected worktree, if any.
    pub fn selected_worktree(&self) -> Option<&Worktree> {
        self.selected_worktree.and_then(|id| self.worktree(id))
    }

    /// The selected tab, if any.
    pub fn selected_tab(&self) -> Option<&Tab> {
        self.selected_tab.and_then(|id| self.tab(id))
    }

    /// Whether the given worktree is the selected one.
    pub fn is_worktree_selected(&self, id: WorktreeId) -> bool {
        self.selected_worktree == Some(id)
    }

    /// Whether the given tab is the selected one.
    pub fn is_tab_selected(&self, id: TabId) -> bool {
        self.selected_tab == Some(id)
    }

    // ------------------------------------------------------------------
    // Project expansion
    // ------------------------------------------------------------------

    /// Expands a project so its worktrees are shown.
    pub fn expand_project(&mut self, project_id: ProjectId) {
        self.expanded_projects.insert(project_id);
    }

    /// Collapses a project so only its row is shown.
    pub fn collapse_project(&mut self, project_id: ProjectId) {
        self.expanded_projects.remove(&project_id);
    }

    /// Flips a project between expanded and collapsed.
    pub fn toggle_project(&mut self, project_id: ProjectId) {
        if !self.expanded_projects.remove(&project_id) {
            self.expanded_projects.insert(project_id);
        }
    }

    /// Whether the project is expanded.
    pub fn is_project_expanded(&self, project_id: ProjectId) -> bool {
        self.expanded_projects.contains(&project_id)
    }

    // ------------------------------------------------------------------
    // Selection
    // ------------------------------------------------------------------

    /// Selects a worktree, clearing any tab selection (a tab selection
    /// implies its worktree is selected; the reverse is not true). Returns
    /// false if no such worktree exists.
    pub fn select_worktree(&mut self, worktree_id: WorktreeId) -> bool {
        if !self
            .worktrees
            .iter()
            .any(|worktree| worktree.id == worktree_id)
        {
            return false;
        }
        self.selected_worktree = Some(worktree_id);
        self.selected_tab = None;
        true
    }

    /// Selects a tab and, implicitly, its parent worktree. Returns false if
    /// no such tab exists.
    pub fn select_tab(&mut self, tab_id: TabId) -> bool {
        let Some(worktree_id) = self.tab(tab_id).map(|tab| tab.worktree_id) else {
            return false;
        };
        self.selected_tab = Some(tab_id);
        self.selected_worktree = Some(worktree_id);
        true
    }

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------

    /// Opens a new tab in the given worktree, returning its id. Returns
    /// `None` if the worktree does not exist.
    pub fn add_tab(
        &mut self,
        worktree_id: WorktreeId,
        title: impl Into<String>,
        kind: TabKind,
    ) -> Option<TabId> {
        if !self
            .worktrees
            .iter()
            .any(|worktree| worktree.id == worktree_id)
        {
            return None;
        }
        let id = TabId::new(self.mint_id());
        self.tabs.push(Tab::new(id, worktree_id, title, kind));
        Some(id)
    }

    /// Closes a tab. Returns false if no such tab exists. If the closed tab
    /// was selected, the selection is cleared.
    pub fn remove_tab(&mut self, tab_id: TabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        self.tabs.remove(index);
        if self.selected_tab == Some(tab_id) {
            self.selected_tab = None;
        }
        true
    }

    // ------------------------------------------------------------------
    // Filtering
    // ------------------------------------------------------------------

    /// Filters the tree by a case-insensitive substring of project names,
    /// worktree branches and tab titles.
    ///
    /// An empty or whitespace-only query returns the whole tree (every
    /// project, with all of its worktrees and all of their tabs).
    ///
    /// With a non-empty query, a project stays visible when its name matches
    /// or any descendant matches:
    ///
    /// - a project whose name matches keeps *all* of its worktrees and all
    ///   of their tabs;
    /// - otherwise a worktree is visible when its branch matches or it owns
    ///   a matching tab — and then keeps all of its tabs only if its branch
    ///   matched, else just the matching tabs;
    /// - a project with no matching name, worktree or tab is dropped.
    ///
    /// The result carries `is_expanded` and selection flags, but expansion
    /// is a rendering decision left to the caller: a collapsed project still
    /// reports its worktrees here.
    pub fn filter(&self, query: &str) -> FilteredTree<'_> {
        let query = query.trim();
        if query.is_empty() {
            return FilteredTree {
                projects: self
                    .projects
                    .iter()
                    .map(|project| FilteredProject {
                        project,
                        is_expanded: self.is_project_expanded(project.id),
                        worktrees: self
                            .worktrees_of(project.id)
                            .map(|worktree| FilteredWorktree {
                                worktree,
                                is_selected: self.is_worktree_selected(worktree.id),
                                tabs: self
                                    .tabs_of(worktree.id)
                                    .map(|tab| FilteredTab {
                                        tab,
                                        is_selected: self.is_tab_selected(tab.id),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            };
        }

        let query = query.to_lowercase();
        let matches = |candidate: &str| candidate.to_lowercase().contains(&query);

        let mut projects = Vec::new();
        for project in &self.projects {
            let project_matches = matches(&project.name);

            let mut worktrees = Vec::new();
            for worktree in self.worktrees_of(project.id) {
                let worktree_matches = matches(&worktree.branch);
                let tabs: Vec<FilteredTab> = self
                    .tabs_of(worktree.id)
                    .filter(|tab| project_matches || worktree_matches || matches(&tab.title))
                    .map(|tab| FilteredTab {
                        tab,
                        is_selected: self.is_tab_selected(tab.id),
                    })
                    .collect();

                if project_matches || worktree_matches || !tabs.is_empty() {
                    worktrees.push(FilteredWorktree {
                        worktree,
                        is_selected: self.is_worktree_selected(worktree.id),
                        tabs,
                    });
                }
            }

            if project_matches || !worktrees.is_empty() {
                projects.push(FilteredProject {
                    project,
                    is_expanded: self.is_project_expanded(project.id),
                    worktrees,
                });
            }
        }

        FilteredTree { projects }
    }

    fn mint_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project_id() -> ProjectId {
        ProjectId::new(1)
    }

    fn worktree_id() -> WorktreeId {
        WorktreeId::new(2)
    }

    /// A workspace with one project ("sirio"), two worktrees ("main",
    /// "feature/login") and tabs ("Terminal", "Chat", "Browser").
    fn seeded_workspace() -> Workspace {
        let mut workspace = Workspace::new();
        workspace.projects.push(Project::new(
            project_id(),
            "sirio",
            PathBuf::from("/Users/me/sirio"),
            true,
        ));
        workspace.worktrees.push(Worktree::new(
            worktree_id(),
            project_id(),
            "main",
            PathBuf::from("/Users/me/sirio"),
            true,
        ));
        workspace.worktrees.push(Worktree::new(
            WorktreeId::new(3),
            project_id(),
            "feature/login",
            PathBuf::from("/Users/me/sirio-feature"),
            false,
        ));
        workspace.tabs.push(Tab::new(
            TabId::new(4),
            worktree_id(),
            "Terminal",
            TabKind::Terminal,
        ));
        workspace.tabs.push(Tab::new(
            TabId::new(5),
            worktree_id(),
            "Chat",
            TabKind::AgentChat,
        ));
        workspace.tabs.push(Tab::new(
            TabId::new(6),
            WorktreeId::new(3),
            "Browser",
            TabKind::Browser,
        ));
        workspace
    }

    #[test]
    fn empty_query_returns_the_whole_tree() {
        let workspace = seeded_workspace();
        let filtered = workspace.filter("   ");

        assert_eq!(filtered.projects.len(), 1);
        let project = &filtered.projects[0];
        assert_eq!(project.project.name, "sirio");
        assert_eq!(project.worktrees.len(), 2);
        assert_eq!(project.worktrees[0].tabs.len(), 2);
        assert_eq!(project.worktrees[1].tabs.len(), 1);
        assert!(!project.is_expanded);
    }

    #[test]
    fn filter_matches_project_name() {
        let workspace = seeded_workspace();
        let filtered = workspace.filter("sir");

        assert_eq!(filtered.projects.len(), 1);
        // The project itself matched, so all descendants stay visible.
        assert_eq!(filtered.projects[0].worktrees.len(), 2);
        assert_eq!(filtered.projects[0].worktrees[0].tabs.len(), 2);
    }

    #[test]
    fn filter_matches_worktree_branch() {
        let workspace = seeded_workspace();
        let filtered = workspace.filter("login");

        assert_eq!(filtered.projects.len(), 1);
        let worktrees = &filtered.projects[0].worktrees;
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].worktree.branch, "feature/login");
        // Branch matched, so all of its tabs stay visible.
        assert_eq!(worktrees[0].tabs.len(), 1);
    }

    #[test]
    fn filter_keeps_project_whose_only_match_is_a_nested_tab_title() {
        let workspace = seeded_workspace();
        let filtered = workspace.filter("chat");

        assert_eq!(
            filtered.projects.len(),
            1,
            "project must survive via its tab"
        );
        let project = &filtered.projects[0];
        assert_eq!(
            project.worktrees.len(),
            1,
            "only the owning worktree is visible"
        );
        let worktree = &project.worktrees[0];
        assert_eq!(worktree.worktree.branch, "main");
        assert_eq!(worktree.tabs.len(), 1, "only the matching tab is visible");
        assert_eq!(worktree.tabs[0].tab.title, "Chat");
    }

    #[test]
    fn filter_is_case_insensitive() {
        let workspace = seeded_workspace();
        let by_title = workspace.filter("CHAT");
        let by_branch = workspace.filter("LOGIN");
        let by_project = workspace.filter("SIRIO");

        assert_eq!(by_title.projects.len(), 1);
        assert_eq!(by_branch.projects.len(), 1);
        assert_eq!(by_project.projects.len(), 1);
    }

    #[test]
    fn filter_drops_projects_with_no_match() {
        let mut workspace = seeded_workspace();
        workspace.projects.push(Project::new(
            ProjectId::new(9),
            "unrelated",
            PathBuf::from("/Users/me/unrelated"),
            false,
        ));

        let filtered = workspace.filter("sirio");
        assert_eq!(filtered.projects.len(), 1);
        assert_eq!(filtered.projects[0].project.name, "sirio");

        let filtered = workspace.filter("unrelated");
        assert_eq!(filtered.projects.len(), 1);
        assert_eq!(filtered.projects[0].project.name, "unrelated");
    }

    #[test]
    fn expansion_toggles_are_idempotent() {
        let mut workspace = seeded_workspace();
        let id = project_id();

        assert!(!workspace.is_project_expanded(id));
        workspace.toggle_project(id);
        assert!(workspace.is_project_expanded(id));
        workspace.toggle_project(id);
        assert!(!workspace.is_project_expanded(id));

        workspace.expand_project(id);
        workspace.expand_project(id);
        assert!(workspace.is_project_expanded(id));
        workspace.collapse_project(id);
        assert!(!workspace.is_project_expanded(id));
    }

    #[test]
    fn selecting_a_worktree_clears_tab_selection() {
        let mut workspace = seeded_workspace();

        assert!(workspace.select_tab(TabId::new(5)));
        assert_eq!(workspace.selected_tab().unwrap().title, "Chat");
        assert_eq!(
            workspace.selected_worktree().unwrap().id,
            worktree_id(),
            "selecting a tab selects its worktree too"
        );

        assert!(workspace.select_worktree(WorktreeId::new(3)));
        assert_eq!(
            workspace.selected_worktree().unwrap().branch,
            "feature/login"
        );
        assert_eq!(
            workspace.selected_tab(),
            None,
            "worktree selection clears the tab"
        );
    }

    #[test]
    fn selecting_unknown_entities_fails() {
        let mut workspace = seeded_workspace();
        assert!(!workspace.select_worktree(WorktreeId::new(999)));
        assert!(!workspace.select_tab(TabId::new(999)));
    }

    #[test]
    fn add_and_remove_tabs() {
        let mut workspace = seeded_workspace();

        let id = workspace
            .add_tab(worktree_id(), "Diff", TabKind::Diff)
            .expect("worktree exists");
        assert_eq!(workspace.tabs_of(worktree_id()).count(), 3);
        assert_eq!(workspace.tab(id).unwrap().kind, TabKind::Diff);

        assert!(workspace.select_tab(id));
        assert!(workspace.remove_tab(id));
        assert_eq!(workspace.tabs_of(worktree_id()).count(), 2);
        assert_eq!(
            workspace.selected_tab(),
            None,
            "closing the selected tab clears it"
        );

        assert!(!workspace.remove_tab(id), "already removed");
    }

    #[test]
    fn add_tab_rejects_unknown_worktree() {
        let mut workspace = seeded_workspace();
        assert_eq!(
            workspace.add_tab(WorktreeId::new(999), "x", TabKind::Terminal),
            None
        );
    }

    #[test]
    fn load_project_fills_model_fields_for_a_git_repo() {
        // Constructed from a DiscoveredProject without touching the
        // filesystem: this test pins the branch-label fallbacks.
        let discovered = discovery::DiscoveredProject {
            is_git: true,
            worktrees: vec![
                DiscoveredWorktree {
                    path: PathBuf::from("/Users/me/sirio"),
                    head: Some("1638c08b4ffb127b9852cbab3124df6a374d07f7".into()),
                    branch: Some("main".into()),
                    is_primary: true,
                    locked: false,
                    prunable: false,
                },
                // Detached linked worktree: labelled by short head.
                DiscoveredWorktree {
                    path: PathBuf::from("/Users/me/sirio-detached"),
                    head: Some("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".into()),
                    branch: None,
                    is_primary: false,
                    locked: false,
                    prunable: false,
                },
            ],
        };

        // Reuse the private discovery entry point via load_project's core by
        // simulating: build the model exactly as load_project would.
        let mut workspace = Workspace::new();
        let project_id = ProjectId::new(workspace.mint_id());
        workspace.projects.push(Project::new(
            project_id,
            "sirio",
            PathBuf::from("/Users/me/sirio"),
            discovered.is_git,
        ));
        for worktree in discovered.worktrees {
            let worktree = workspace.worktree_from_discovered(worktree, project_id);
            workspace.worktrees.push(worktree);
        }

        let project = workspace.project(project_id).unwrap();
        assert!(project.is_git);
        assert_eq!(project.name, "sirio");

        let worktrees: Vec<_> = workspace.worktrees_of(project_id).collect();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch, "main");
        assert!(worktrees[0].is_primary);
        assert_eq!(
            worktrees[1].branch, "a1b2c3d",
            "detached linked worktree shows short head"
        );
        assert!(!worktrees[1].is_primary);
    }

    #[test]
    fn ids_are_unique_across_loads_and_tabs() {
        let mut workspace = Workspace::new();
        // Loading needs a real (or here: nonexistent) path; exercise mint_id
        // through add_tab on a hand-seeded worktree instead.
        workspace.projects.push(Project::new(
            project_id(),
            "p",
            PathBuf::from("/tmp/p"),
            false,
        ));
        workspace.worktrees.push(Worktree::new(
            worktree_id(),
            project_id(),
            "main",
            PathBuf::from("/tmp/p"),
            true,
        ));

        let a = workspace
            .add_tab(worktree_id(), "one", TabKind::Terminal)
            .unwrap();
        let b = workspace
            .add_tab(worktree_id(), "two", TabKind::Terminal)
            .unwrap();
        assert_ne!(a, b);
    }
}

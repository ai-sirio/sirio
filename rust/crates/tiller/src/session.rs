//! Session persistence: the shell's layout saved as the user works, and
//! restored at launch.
//!
//! The shell is a single-worktree app: one working directory, one sidebar
//! project, one tab strip. What must survive a relaunch is that layout —
//! the project and worktree records (with stable, path-derived ids so
//! launching in different directories never clobbers each other), the open
//! tabs in order with the active one, and the sidebar selection. That is
//! what [`SessionStore`] writes and [`restore`] reads back.
//!
//! # Database location
//!
//! The session database is scoped per checkout whenever the running binary
//! demonstrably lives inside a git checkout — any development build, debug
//! or release; an installed binary keeps the single stable user-wide
//! location. See [`database_path`] for the full resolution order.
//!
//! # Failure behaviour
//!
//! A missing, corrupt, or newer-schema database is never fatal: the store
//! logs the reason, keeps running in fallback mode (all writes become
//! no-ops), and [`restore`] returns the default layout. An app that refuses
//! to open because of its own state file is worse than one that forgets the
//! layout.
//!
//! # Debounce
//!
//! Every tab mutation schedules a snapshot; a background thread flushes at
//! most once per [`DEBOUNCE_INTERVAL`], coalescing a burst of changes into
//! one transactional write. `flush_now` covers quit: the window-closed hook
//! calls it so the last action is never lost to the debounce window.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tiller_persistence::{
    AppDatabase, AppSettings, PersistenceError, ProjectRecord, SidebarState, TabRecord,
    TabStateRecord, WorktreeRecord,
};
use tiller_project::{DiscoveredProject, discover_project};

/// How long a burst of changes is held before one write. 500 ms is under the
/// reaction time between discrete user actions (a click then flushes at the
/// next quiet boundary) yet long enough that a burst of programmatic changes
/// collapses into a single write; `flush_now` on quit makes the window
/// lossless.
pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

/// Per-pane scrollback is bounded so a long-running terminal cannot make the
/// session database grow without limit. The renderer-owned capture seam is
/// still supplied by `tiller_terminal`; this is the persistence-side bound.
pub const SCROLLBACK_LIMIT: usize = 256 * 1024;

/// A pane mutation that can be replayed through the public `PaneNode` API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneEvent {
    Split {
        focused: usize,
        new_id: usize,
        direction: String,
    },
    SetRatio {
        path: Vec<bool>,
        ratio_millis: u16,
    },
    Close {
        id: usize,
    },
}

/// Opaque per-tab application state. The event history lets the host restore
/// a generic `PaneNode` without exposing its private fields to persistence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTabState {
    #[serde(default)]
    pub root_id: Option<usize>,
    pub pane_events: Vec<PaneEvent>,
    #[serde(default)]
    pub scrollback: BTreeMap<usize, Vec<u8>>,
}

impl SessionTabState {
    pub fn with_root(root_id: usize) -> Self {
        Self {
            root_id: Some(root_id),
            ..Self::default()
        }
    }

    pub fn bounded_scrollback(bytes: &[u8]) -> Vec<u8> {
        let start = bytes.len().saturating_sub(SCROLLBACK_LIMIT);
        bytes[start..].to_vec()
    }

    fn encode(&self) -> String {
        let bounded = Self {
            root_id: self.root_id,
            pane_events: self.pane_events.clone(),
            scrollback: self
                .scrollback
                .iter()
                .map(|(pane_id, bytes)| (*pane_id, Self::bounded_scrollback(bytes)))
                .collect(),
        };
        serde_json::to_string(&bounded).expect("session tab state is serializable")
    }

    fn decode(raw: &str) -> Result<Self, serde_json::Error> {
        let mut state: Self = serde_json::from_str(raw)?;
        for bytes in state.scrollback.values_mut() {
            let bounded = Self::bounded_scrollback(bytes);
            *bytes = bounded;
        }
        Ok(state)
    }
}

/// How often the flusher thread checks for pending work. A parked thread
/// polling a mutex every 25 ms costs nothing.
const FLUSH_POLL: Duration = Duration::from_millis(25);

/// Where this process's session database lives.
///
/// Resolution order:
/// 1. `$TILLER_DB` — explicit override (tests, demos, a power user).
/// 2. A git checkout containing the running binary: the database is scoped
///    to that checkout. Several checkouts of the app on one machine
///    (worktrees, agents building each of them) can never write each
///    other's session state, and no development build can ever touch a
///    database belonging to another checkout or to an installed copy.
/// 3. (debug builds only) A git checkout containing the process working
///    directory — for builds whose target directory is shared outside the
///    checkout (`CARGO_TARGET_DIR`).
/// 4. The stable user-wide location. An installed binary — inside an .app
///    bundle, in `~/.cargo/bin`, anywhere without a `.git` ancestor — always
///    lands here, so a shipped app keeps exactly one database location
///    across upgrades.
///
/// On macOS the stable path deliberately does NOT point at the shipping Swift
/// app's "Application Support/Tiller" database: pointing the rewrite at it opened
/// a 21 MB production file, ran its own migrations inside it, and left two
/// foreign tables behind. The rewrite keeps its own state until it replaces
/// the Swift app outright. On Linux the stable root is the XDG state directory.
pub fn database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("TILLER_DB") {
        return PathBuf::from(path);
    }
    let root = app_support_root();
    let exe = std::env::current_exe().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let path = database_path_for(&root, &exe, &cwd, cfg!(debug_assertions));
    if path != root.join("tiller.sqlite") {
        eprintln!(
            "[session] development build: session state is checkout-local at {}",
            path.display()
        );
    }
    path
}

/// The root under which every TillerRust database lives: the stable
/// `tiller.sqlite` for installed binaries plus a `checkouts/` subtree with
/// one directory per development checkout. Linux uses XDG_STATE_HOME (or
/// `$HOME/.local/state`); macOS retains Application Support.
fn app_support_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/TillerRust"))
            .unwrap_or_else(|| std::env::temp_dir().join("TillerRust"));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let environment: std::collections::BTreeMap<String, String> = std::env::vars().collect();
        app_support_root_for(&environment)
    }
}

fn app_support_root_for(environment: &std::collections::BTreeMap<String, String>) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        return environment
            .get("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/TillerRust"))
            .unwrap_or_else(|| std::env::temp_dir().join("TillerRust"));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let state_home = environment
            .get("XDG_STATE_HOME")
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| {
                environment
                    .get("HOME")
                    .map(Path::new)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".local/state"))
            })
            .unwrap_or_else(std::env::temp_dir);
        state_home.join("TillerRust")
    }
}

/// Resolves where this process's session database lives.
///
/// - `$TILLER_DB` always wins and is used verbatim (handled by the caller).
/// - Otherwise, if the running binary — or, for debug builds, the process
///   working directory — sits inside a git checkout, the database is scoped
///   to that checkout: every checkout of the app on a machine gets its own
///   state, and no development build can ever touch another checkout's
///   database or the installed app's.
/// - Otherwise the stable user-wide path is used.
fn database_path_for(
    app_support_root: &Path,
    exe: &Path,
    cwd: &Path,
    use_cwd_fallback: bool,
) -> PathBuf {
    if let Some(checkout) = find_checkout_root(exe) {
        return scoped_database_path(app_support_root, &checkout);
    }
    if use_cwd_fallback && let Some(checkout) = find_checkout_root(cwd) {
        return scoped_database_path(app_support_root, &checkout);
    }
    app_support_root.join("tiller.sqlite")
}

/// Walks up from `start` and returns the first ancestor that is a git
/// checkout root — a directory containing a `.git` entry, file or directory
/// (a `git worktree` carries `.git` as a file). `None` when no checkout
/// contains `start`.
fn find_checkout_root(start: &Path) -> Option<PathBuf> {
    let canonical = start.canonicalize().ok()?;
    canonical
        .ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Scoped database for one checkout:
/// `checkouts/<basename>-<hash>/tiller.sqlite` under the app-support root.
/// The hash covers the canonical checkout root, so two checkouts with the
/// same directory name differ, while symlinked views of one checkout
/// collide correctly.
fn scoped_database_path(app_support_root: &Path, checkout_root: &Path) -> PathBuf {
    let basename = checkout_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hash = fnv1a64(checkout_root.to_string_lossy().as_bytes()) & 0xffff_ffff;
    app_support_root
        .join("checkouts")
        .join(format!(
            "{}-{hash:08x}",
            sanitize_filename_component(&basename)
        ))
        .join("tiller.sqlite")
}

/// Keeps a directory-name component filesystem-safe and readable: `-_.` and
/// alphanumerics survive; everything else becomes `_`.
fn sanitize_filename_component(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// One persisted tab of the shell layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTab {
    /// Stable database identity. Unlike the tab's array position, this does
    /// not change when the user reorders tabs.
    pub id: String,
    /// Tab title.
    pub title: String,
    /// Surface kind: "chat", "terminal", or "diff" (the shell's `TabKind`).
    pub kind: String,
    /// Stable adapter identity for an agent-backed tab.
    pub agent_id: Option<String>,
    /// Whether this tab is the active one.
    pub active: bool,
}

/// The shell layout as persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLayout {
    /// The single worktree directory.
    pub working_directory: PathBuf,
    /// The worktree's branch label ("main" when not a repository).
    pub branch: String,
    /// The open tabs, in strip order.
    pub tabs: Vec<SessionTab>,
    /// State aligned by tab index; missing entries mean a fresh tab state.
    pub tab_states: Vec<SessionTabState>,
}

impl SessionLayout {
    /// A fresh default: one chat tab and one terminal, as the shell always
    /// started before persistence existed.
    pub fn default_in(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            branch: String::new(),
            tabs: vec![
                SessionTab {
                    id: "default-chat".into(),
                    title: "Chat".into(),
                    kind: "chat".into(),
                    agent_id: None,
                    active: false,
                },
                SessionTab {
                    id: "default-terminal".into(),
                    title: "Terminal".into(),
                    kind: "terminal".into(),
                    agent_id: None,
                    active: true,
                },
            ],
            tab_states: vec![SessionTabState::default(), SessionTabState::default()],
        }
    }
}

/// The result of restoring a layout at launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredSession {
    /// The worktree directory to root the shell in.
    pub working_directory: PathBuf,
    /// Tabs to rebuild, in strip order.
    pub tabs: Vec<SessionTab>,
    /// State aligned by tab index; malformed or missing rows are defaulted.
    pub tab_states: Vec<SessionTabState>,
    /// Human-readable notes about what was skipped or repaired.
    pub diagnostics: Vec<String>,
}

/// A project and the worktrees discovered beneath it, ready for the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogProject {
    pub id: String,
    pub name: String,
    pub root_path: PathBuf,
    pub is_git: bool,
    pub worktrees: Vec<CatalogWorktree>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogWorktree {
    pub branch: String,
    pub path: PathBuf,
    pub is_primary: bool,
}

/// Project-level settings that are not derived from the filesystem. Kept
/// separate from [`CatalogProject`] so discovery can rebuild worktrees
/// without losing user-authored identity settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogProjectSettings {
    pub color_hex: Option<String>,
    pub display_name: Option<String>,
    pub icon_kind: String,
    pub icon_value: Option<String>,
}

impl Default for CatalogProjectSettings {
    fn default() -> Self {
        Self {
            color_hex: None,
            display_name: None,
            icon_kind: "icon".to_string(),
            icon_value: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectCatalog {
    projects: Vec<CatalogProject>,
    settings: BTreeMap<String, CatalogProjectSettings>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredCatalog {
    pub projects: Vec<CatalogProject>,
    pub settings: BTreeMap<String, CatalogProjectSettings>,
    pub diagnostics: Vec<String>,
}

impl ProjectCatalog {
    pub fn from_projects(projects: Vec<CatalogProject>) -> Self {
        let settings = projects
            .iter()
            .map(|project| (project.id.clone(), CatalogProjectSettings::default()))
            .collect();
        Self { projects, settings }
    }

    pub fn from_restored(
        projects: Vec<CatalogProject>,
        settings: BTreeMap<String, CatalogProjectSettings>,
    ) -> Self {
        let mut catalog = Self::from_projects(projects);
        for (id, settings) in settings {
            if catalog.projects.iter().any(|project| project.id == id) {
                catalog.settings.insert(id, settings);
            }
        }
        catalog
    }

    pub fn projects(&self) -> &[CatalogProject] {
        &self.projects
    }

    pub fn project_settings(&self, id: &str) -> CatalogProjectSettings {
        self.settings.get(id).cloned().unwrap_or_default()
    }

    pub fn update_project_settings(
        &mut self,
        id: &str,
        settings: CatalogProjectSettings,
    ) -> Result<(), String> {
        if !self.projects.iter().any(|project| project.id == id) {
            return Err(format!("unknown project: {id}"));
        }
        self.settings.insert(id.to_string(), settings);
        Ok(())
    }

    /// Replaces discovered projects while retaining identity settings for
    /// stable ids. New projects receive defaults; removed projects release
    /// their settings entry.
    pub fn replace_projects(&mut self, projects: Vec<CatalogProject>) {
        let old_settings = std::mem::take(&mut self.settings);
        self.projects = projects;
        self.settings = self
            .projects
            .iter()
            .map(|project| {
                (
                    project.id.clone(),
                    old_settings.get(&project.id).cloned().unwrap_or_default(),
                )
            })
            .collect();
    }

    /// Moves a project in catalog order. The sidebar keeps each project's
    /// worktrees attached to its project block; only the host catalog owns
    /// the durable order.
    pub fn reorder_projects(&mut self, from: usize, target: usize, before: bool) -> bool {
        reorder_item(&mut self.projects, from, target, before)
    }

    /// Moves one worktree within its owning project. Cross-project movement
    /// is intentionally not representable through this API.
    pub fn reorder_worktrees(
        &mut self,
        project_index: usize,
        from: usize,
        target: usize,
        before: bool,
    ) -> bool {
        self.projects
            .get_mut(project_index)
            .is_some_and(|project| reorder_item(&mut project.worktrees, from, target, before))
    }

    pub fn add(&mut self, path: &Path) -> Result<bool, String> {
        let input_path = path
            .canonicalize()
            .map_err(|error| format!("cannot add {}: {error}", path.display()))?;
        if !input_path.is_dir() {
            return Err(format!("{} is not a directory", input_path.display()));
        }
        let discovered = discover_project(&input_path).map_err(|error| error.to_string())?;
        let root_path = catalog_root(&input_path, &discovered);
        if self.projects.iter().any(|project| {
            root_path == project.root_path
                || root_path.starts_with(&project.root_path)
                || project.root_path.starts_with(&root_path)
        }) {
            return Ok(false);
        }

        let project = catalog_project(&root_path, discovered);
        self.settings
            .insert(project.id.clone(), CatalogProjectSettings::default());
        self.projects.push(project);
        Ok(true)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let Some(index) = self.projects.iter().position(|project| project.id == id) else {
            return false;
        };
        self.projects.remove(index);
        self.settings.remove(id);
        true
    }

    /// Re-discovers one project after an external repository transition such
    /// as `git init`, preserving its stable catalog identity.
    pub fn refresh_project(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .projects
            .iter()
            .position(|project| project.id == id)
            .ok_or_else(|| format!("unknown project: {id}"))?;
        let root = self.projects[index].root_path.clone();
        let discovered = discover_project(&root).map_err(|error| error.to_string())?;
        let mut replacement = catalog_project(&root, discovered);
        replacement.id = id.to_string();
        self.projects[index] = replacement;
        Ok(())
    }

    /// Changes the application-level primary marker for a worktree. Git's
    /// physical primary checkout is not moved; this is sidebar metadata that
    /// controls the `Set Primary` / `Unset Primary` affordance.
    pub fn set_primary(&mut self, path: &Path, primary: bool) -> Result<(), String> {
        let Some(project_index) = self.projects.iter().position(|project| {
            project
                .worktrees
                .iter()
                .any(|worktree| worktree.path == path)
        }) else {
            return Err(format!("unknown worktree: {}", path.display()));
        };
        let Some(worktree_index) = self.projects[project_index]
            .worktrees
            .iter()
            .position(|worktree| worktree.path == path)
        else {
            return Err(format!("unknown worktree: {}", path.display()));
        };
        if primary {
            for worktree in &mut self.projects[project_index].worktrees {
                worktree.is_primary = false;
            }
        }
        self.projects[project_index].worktrees[worktree_index].is_primary = primary;
        Ok(())
    }
}

fn reorder_item<T>(items: &mut Vec<T>, from: usize, target: usize, before: bool) -> bool {
    if from >= items.len() || target >= items.len() || from == target {
        return false;
    }
    let item = items.remove(from);
    let target_after_remove = if from < target { target - 1 } else { target };
    let insert_at = target_after_remove + usize::from(!before);
    items.insert(insert_at.min(items.len()), item);
    true
}

fn catalog_project(root_path: &Path, discovered: DiscoveredProject) -> CatalogProject {
    let root_path = catalog_root(root_path, &discovered);
    let name = root_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| root_path.to_string_lossy().into_owned());
    let id = project_id(&root_path);
    let worktrees = discovered
        .worktrees
        .into_iter()
        .map(|worktree| CatalogWorktree {
            branch: worktree.branch.unwrap_or_else(|| {
                if worktree.is_primary {
                    "main".into()
                } else {
                    worktree
                        .head
                        .as_deref()
                        .map(|head| head.chars().take(7).collect())
                        .unwrap_or_else(|| "HEAD".into())
                }
            }),
            path: canonical_path(&worktree.path),
            is_primary: worktree.is_primary,
        })
        .collect();
    CatalogProject {
        id,
        name,
        root_path: root_path.to_path_buf(),
        is_git: discovered.is_git,
        worktrees,
    }
}

/// FNV-1a 64-bit hash — stable across platforms and runs (never randomized).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Stable, path-derived ids for the shell's single project and worktree.
/// Different directories get different ids, so several sessions can coexist
/// in one database without clobbering each other's records; the same
/// directory always resolves to the same ids across launches.
fn canonical_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The project identity is the primary checkout's path, not whichever linked
/// worktree happened to be open when the catalog was saved. Git reports the
/// primary worktree first, so this also works when the input itself has a
/// `.git` file and is a linked checkout.
fn catalog_root(input_path: &Path, discovered: &DiscoveredProject) -> PathBuf {
    discovered
        .worktrees
        .iter()
        .find(|worktree| worktree.is_primary)
        .map(|worktree| canonical_path(&worktree.path))
        .unwrap_or_else(|| canonical_path(input_path))
}

fn project_id(root_path: &Path) -> String {
    let hash = fnv1a64(canonical_path(root_path).to_string_lossy().as_bytes());
    format!("p-{hash:016x}")
}

fn worktree_id(project_id: &str, index: usize) -> String {
    format!("{project_id}-wt-{index}")
}

/// One id convention for every persisted catalog worktree. Existing callers
/// that only have a working directory are resolved through Git so a linked
/// worktree still gets the primary project's id and its stable list index.
fn catalog_ids_for_path(working_directory: &Path) -> (PathBuf, String, String) {
    let working_directory = canonical_path(working_directory);
    if let Ok(discovered) = discover_project(&working_directory)
        && discovered.is_git
        && !discovered.worktrees.is_empty()
    {
        let root = catalog_root(&working_directory, &discovered);
        let project_id = project_id(&root);
        let index = discovered
            .worktrees
            .iter()
            .position(|worktree| canonical_path(&worktree.path) == working_directory)
            .unwrap_or(0);
        return (root, project_id.clone(), worktree_id(&project_id, index));
    }

    let project_id = project_id(&working_directory);
    (
        working_directory,
        project_id.clone(),
        worktree_id(&project_id, 0),
    )
}

/// The stable persisted worktree identity used by tab and chat records.
pub fn persisted_worktree_id(working_directory: &Path) -> String {
    catalog_ids_for_path(working_directory).2
}

/// Allocates a new tab identity. The timestamp prevents reuse after a tab is
/// closed while the counter keeps same-millisecond allocations distinct.
pub fn new_tab_id(working_directory: &Path, counter: usize) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-tab-{timestamp:x}-{counter}",
        persisted_worktree_id(working_directory)
    )
}

/// Writes one layout to the database: the project and worktree records
/// (upserted), the worktree's tabs (replaced atomically, with the active
/// flag normalized), and the sidebar selection (read-modify-write so other
/// projects' expansion state survives).
fn write_layout(db: &AppDatabase, layout: &SessionLayout) -> Result<(), PersistenceError> {
    let (project_root, project_id, worktree_id) = catalog_ids_for_path(&layout.working_directory);
    let name = project_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_root.to_string_lossy().into_owned());
    let project_path = project_root.to_string_lossy().into_owned();
    let worktree_path = layout.working_directory.to_string_lossy().into_owned();
    let branch = if layout.branch.is_empty() {
        "main".to_string()
    } else {
        layout.branch.clone()
    };

    let mut project = db
        .projects()?
        .into_iter()
        .find(|project| project.id == project_id)
        .unwrap_or_else(|| ProjectRecord::new(&project_id, &name, &project_path));
    project.name = name;
    project.root_path = project_path;
    db.save_project(&project)?;
    db.save_worktree(&WorktreeRecord::new(
        &worktree_id,
        &project_id,
        &branch,
        &worktree_path,
    ))?;

    let tabs: Vec<TabRecord> = layout
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| TabRecord {
            id: if tab.id.is_empty() {
                format!("{worktree_id}-tab-{index}")
            } else {
                tab.id.clone()
            },
            worktree_id: worktree_id.clone(),
            title: tab.title.clone(),
            kind: tab.kind.clone(),
            agent_id: tab.agent_id.clone(),
            order_idx: index as i64,
            is_active: tab.active,
        })
        .collect();
    db.save_tabs(&worktree_id, &tabs)?;
    let states: Vec<TabStateRecord> = layout
        .tab_states
        .iter()
        .enumerate()
        .take(layout.tabs.len())
        .map(|(index, state)| {
            TabStateRecord::new(
                layout
                    .tabs
                    .get(index)
                    .map(|tab| {
                        if tab.id.is_empty() {
                            format!("{worktree_id}-tab-{index}")
                        } else {
                            tab.id.clone()
                        }
                    })
                    .unwrap_or_else(|| format!("{worktree_id}-tab-{index}")),
                state.encode(),
            )
        })
        .collect();
    db.save_tab_states(&worktree_id, &states)?;

    // Sidebar selection: expand our project, select our worktree, and keep
    // every other project's expansion state intact.
    let mut state = db.sidebar_state().unwrap_or(SidebarState {
        expanded_project_ids: Vec::new(),
        selected_worktree_id: None,
    });
    state.selected_worktree_id = Some(worktree_id.clone());
    if !state.expanded_project_ids.contains(&project_id) {
        state.expanded_project_ids.push(project_id);
    }
    db.save_sidebar_state(&state)?;
    Ok(())
}

fn write_catalog(db: &AppDatabase, catalog: &ProjectCatalog) -> Result<(), PersistenceError> {
    let desired_ids: std::collections::HashSet<&str> = catalog
        .projects
        .iter()
        .map(|project| project.id.as_str())
        .collect();
    for project in db.projects()? {
        if !desired_ids.contains(project.id.as_str()) {
            db.remove_project(&project.id)?;
        }
    }

    for (project_index, project) in catalog.projects.iter().enumerate() {
        let mut record = ProjectRecord::new(
            &project.id,
            &project.name,
            project.root_path.to_string_lossy(),
        );
        record.order_idx = project_index as i64;
        let settings = catalog.project_settings(&project.id);
        record.color_hex = settings.color_hex;
        record.display_name = settings.display_name;
        record.icon_kind = settings.icon_kind;
        record.icon_value = settings.icon_value;
        db.save_project(&record)?;

        let desired_worktree_ids: std::collections::HashSet<String> = project
            .worktrees
            .iter()
            .enumerate()
            .map(|(index, _)| worktree_id(&project.id, index))
            .collect();
        if project.is_git {
            for worktree in db.worktrees_of_project(&project.id)? {
                if !desired_worktree_ids.contains(&worktree.id) {
                    db.remove_worktree(&worktree.id)?;
                }
            }
        }
        for (worktree_index, worktree) in project.worktrees.iter().enumerate() {
            let mut record = WorktreeRecord::new(
                worktree_id(&project.id, worktree_index),
                &project.id,
                &worktree.branch,
                worktree.path.to_string_lossy(),
            );
            record.order_idx = worktree_index as i64;
            record.is_primary = worktree.is_primary;
            db.save_worktree(&record)?;
        }
    }
    Ok(())
}

pub fn restore_catalog(database: &Path) -> RestoredCatalog {
    let db = match AppDatabase::open(database) {
        Ok(db) => db,
        Err(error) => {
            return RestoredCatalog {
                projects: Vec::new(),
                settings: BTreeMap::new(),
                diagnostics: vec![format!("project database unavailable: {error}")],
            };
        }
    };
    let mut projects = Vec::new();
    let mut settings = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut seen_project_ids = HashSet::new();
    for record in db.projects().unwrap_or_default() {
        let root = PathBuf::from(&record.root_path);
        if !root.is_dir() {
            diagnostics.push(format!(
                "project {} vanished: {}",
                record.name,
                root.display()
            ));
            continue;
        }
        match discover_project(&root) {
            Ok(discovered) => {
                let project = catalog_project(&root, discovered);
                if seen_project_ids.insert(project.id.clone()) {
                    settings.insert(
                        project.id.clone(),
                        CatalogProjectSettings {
                            color_hex: record.color_hex.clone(),
                            display_name: record.display_name.clone(),
                            icon_kind: record.icon_kind.clone(),
                            icon_value: record.icon_value.clone(),
                        },
                    );
                    projects.push(project);
                } else {
                    diagnostics.push(format!(
                        "project {} duplicates an existing repository and was merged",
                        record.name
                    ));
                }
            }
            Err(error) => diagnostics.push(format!(
                "project {} could not be discovered: {error}",
                record.name
            )),
        }
    }
    RestoredCatalog {
        projects,
        settings,
        diagnostics,
    }
}

/// Reads the persisted layout back. Never fails: any database problem is
/// logged and yields the default layout.
pub fn restore(database: &Path, fallback_directory: &Path) -> RestoredSession {
    // First launch has no directory yet, and opening into a missing one just
    // fails. Create it here too rather than only on the write path.
    if let Some(parent) = database.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = match AppDatabase::open(database) {
        Ok(db) => db,
        Err(error) => {
            eprintln!("[session] database unavailable ({error}); using the default layout");
            let mut fallback = default_restored(fallback_directory);
            fallback
                .diagnostics
                .push(format!("database unavailable: {error}"));
            return fallback;
        }
    };
    restore_from(&db, fallback_directory)
}

fn restore_from(db: &AppDatabase, fallback_directory: &Path) -> RestoredSession {
    let mut diagnostics = Vec::new();

    // Which worktree was selected? Fall back to the first one, then to the
    // default layout when the database holds no layout at all.
    let state = db.sidebar_state().unwrap_or(SidebarState {
        expanded_project_ids: Vec::new(),
        selected_worktree_id: None,
    });
    let worktrees = db.worktrees().unwrap_or_default();
    let worktree = state
        .selected_worktree_id
        .as_ref()
        .and_then(|id| worktrees.iter().find(|worktree| &worktree.id == id))
        .or_else(|| worktrees.first());
    let Some(worktree) = worktree else {
        return default_restored(fallback_directory);
    };

    let working_directory = PathBuf::from(&worktree.path);
    if !working_directory.is_dir() {
        diagnostics.push(format!(
            "worktree {} ({}) no longer exists; falling back to the current directory",
            worktree.branch, worktree.path
        ));
        return default_restored(fallback_directory);
    }

    let records = match db.tabs_of_worktree(&worktree.id) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("[session] failed to read tabs: {error}; using the default layout");
            return default_restored(fallback_directory);
        }
    };

    let state_records = match db.tab_states_of_worktree(&worktree.id) {
        Ok(records) => records
            .into_iter()
            .map(|record| (record.tab_id, record.state))
            .collect::<BTreeMap<_, _>>(),
        Err(error) => {
            diagnostics.push(format!("failed to read tab state: {error}"));
            BTreeMap::new()
        }
    };
    let mut tabs = Vec::new();
    let mut tab_states = Vec::new();
    for record in records {
        // Only editor tabs remain unavailable in this shell; browser is a
        // first-class persisted surface alongside chat, terminal, and diff.
        if record.kind != "chat"
            && record.kind != "terminal"
            && record.kind != "diff"
            && record.kind != "browser"
        {
            diagnostics.push(format!(
                "tab {:?} ({}) is not restorable in this build; skipped",
                record.title, record.kind
            ));
            continue;
        }
        tabs.push(SessionTab {
            id: record.id,
            title: record.title,
            kind: record.kind,
            agent_id: record.agent_id,
            active: record.is_active,
        });
        let last_tab = tabs.last().expect("just pushed");
        let tab_title = last_tab.title.as_str();
        let tab_state = match state_records.get(&last_tab.id) {
            Some(raw) => match SessionTabState::decode(raw) {
                Ok(state) => state,
                Err(error) => {
                    diagnostics.push(format!(
                        "tab state for {tab_title:?} is invalid; using empty pane state: {error}"
                    ));
                    SessionTabState::default()
                }
            },
            None => SessionTabState::default(),
        };
        tab_states.push(tab_state);
    }

    // The invariant is at most one active tab; if the stored layout has
    // none, the first tab is active, exactly like a fresh shell.
    if !tabs.is_empty() && !tabs.iter().any(|tab| tab.active) {
        tabs[0].active = true;
    }

    RestoredSession {
        working_directory,
        tabs,
        tab_states,
        diagnostics,
    }
}

fn default_restored(fallback_directory: &Path) -> RestoredSession {
    RestoredSession {
        working_directory: fallback_directory.to_path_buf(),
        tabs: SessionLayout::default_in(fallback_directory.to_path_buf()).tabs,
        tab_states: SessionLayout::default_in(fallback_directory.to_path_buf()).tab_states,
        diagnostics: Vec::new(),
    }
}

/// The debounced writer. Cloneable: every clone shares the same database
/// slot, pending snapshot and write counter.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    /// `None` when the database failed to open: the store runs in fallback
    /// mode and every flush is a no-op.
    db: Mutex<Option<AppDatabase>>,
    /// The most recent scheduled layout; a burst coalesces into one write.
    pending: Mutex<Option<SessionLayout>>,
    /// When the next flush may happen (debounce deadline).
    next_flush: Mutex<Instant>,
    /// How many real database writes have happened (the debounce test's
    /// observable).
    writes: AtomicUsize,
    interval: Duration,
}

impl SessionStore {
    /// Opens (or fails into fallback mode) the database at `path` and spawns
    /// the flusher thread. The database file's parent directory is created.
    pub fn open(path: &Path) -> Self {
        Self::open_with(path, DEBOUNCE_INTERVAL)
    }

    /// Like [`SessionStore::open`], with a configurable debounce interval
    /// (tests use a short one).
    pub fn open_with(path: &Path, interval: Duration) -> Self {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = std::fs::create_dir_all(parent);
        }
        let db = match AppDatabase::open(path) {
            Ok(db) => Some(db),
            Err(error) => {
                eprintln!(
                    "[session] cannot open {} ({error}); continuing without persistence",
                    path.display()
                );
                None
            }
        };
        let store = Self {
            inner: Arc::new(SessionInner {
                db: Mutex::new(db),
                pending: Mutex::new(None),
                next_flush: Mutex::new(Instant::now()),
                writes: AtomicUsize::new(0),
                interval,
            }),
        };
        store.spawn_flusher();
        store
    }

    fn spawn_flusher(&self) {
        // The flusher holds a *weak* reference to the shared state, which
        // is its shutdown path: when the last `SessionStore` handle is
        // dropped (the app quits), the upgrade fails and the thread exits
        // instead of running forever. While any handle lives, the debounce
        // behaviour is exactly as before — the weak upgrade only fails at
        // teardown.
        let inner = Arc::downgrade(&self.inner);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(FLUSH_POLL);
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                flush_if_due(&inner);
            }
        });
    }

    /// Records the current layout; the next flush after the debounce
    /// interval writes it. Cheap: just swaps a snapshot.
    pub fn schedule(&self, layout: SessionLayout) {
        *self
            .inner
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(layout);
    }

    /// Persists the user's complete project catalog while preserving tabs on
    /// worktrees that are still present. Project discovery happens before this
    /// method is called, so the database write remains small and deterministic.
    pub fn schedule_catalog(&self, catalog: &ProjectCatalog) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match write_catalog(db, catalog) {
            Ok(()) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                eprintln!("[session] failed to persist project catalog: {error}");
            }
        }
    }

    /// Loads the durable application settings, or their defaults when the
    /// database is unavailable or contains an invalid value.
    ///
    /// This is the persistence seam for the host/UI layer. The settings
    /// editor owns its in-memory values; callers must invoke
    /// [`SessionStore::save_settings`] after a user-visible change.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn load_settings(&self) -> AppSettings {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return AppSettings::default();
        };
        match db.settings() {
            Ok(settings) => settings,
            Err(error) => {
                eprintln!("[session] failed to load settings: {error}");
                AppSettings::default()
            }
        }
    }

    /// Saves the complete application settings in one transaction.
    ///
    /// Persistence failures are logged and do not make the UI fail. This
    /// matches the session layout's best-effort failure policy.
    pub fn save_settings(&self, settings: &AppSettings) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match db.save_settings(settings) {
            Ok(()) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                eprintln!("[session] failed to persist settings: {error}");
            }
        }
    }

    /// Loads pane-to-agent-session associations, or an empty map when the
    /// database is unavailable.
    pub fn load_session_refs(&self) -> BTreeMap<String, String> {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return BTreeMap::new();
        };
        match db.session_refs() {
            Ok(references) => references,
            Err(error) => {
                eprintln!("[session] failed to load session references: {error}");
                BTreeMap::new()
            }
        }
    }

    /// Saves one pane-to-agent-session association immediately. This is a
    /// hook write rather than a layout snapshot, so it must not wait for the
    /// layout debounce window.
    pub fn save_session_ref(&self, session: &str, reference: &str) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match db.save_session_ref(session, reference) {
            Ok(()) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                eprintln!("[session] failed to persist session reference: {error}");
            }
        }
    }

    /// Loads browser-origin grants for new browser surfaces.
    pub fn load_browser_origin_grants(&self) -> Vec<String> {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return Vec::new();
        };
        match db.browser_origin_grants() {
            Ok(origins) => origins,
            Err(error) => {
                eprintln!("[session] failed to load browser origin grants: {error}");
                Vec::new()
            }
        }
    }

    /// Persists one browser-origin grant immediately after the user allows it.
    pub fn save_browser_origin_grant(&self, origin: &str) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match db.save_browser_origin_grant(origin) {
            Ok(()) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                eprintln!("[session] failed to persist browser origin grant: {error}");
            }
        }
    }

    /// Revokes one browser-origin grant from durable session state.
    pub fn revoke_browser_origin(&self, origin: &str) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match db.revoke_browser_origin(origin) {
            Ok(true) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!("[session] failed to revoke browser origin grant: {error}");
            }
        }
    }

    /// Revokes all browser-origin grants from durable session state.
    pub fn revoke_all_browser_origins(&self) {
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match db.revoke_all_browser_origins() {
            Ok(count) if count > 0 => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("[session] failed to revoke all browser origin grants: {error}");
            }
        }
    }

    /// The number of real database writes performed so far.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn writes(&self) -> usize {
        self.inner.writes.load(Ordering::Relaxed)
    }

    /// Flushes the pending layout immediately (synchronously), ignoring the
    /// debounce window. Called on window close so quitting never loses the
    /// last change.
    pub fn flush_now(&self) {
        *self
            .inner
            .next_flush
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
        flush_if_due(&self.inner);
    }

    /// Whether the database is usable (false in fallback mode).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn persistence_enabled(&self) -> bool {
        self.inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }
}

/// One debounce-checked flush: writes the pending layout when one exists and
/// the interval has elapsed since the last flush. Never panics; database
/// errors are logged and the pending snapshot is dropped (the next change
/// retries).
fn flush_if_due(inner: &SessionInner) {
    let (due, layout) = {
        let mut pending = inner
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut next_flush = inner
            .next_flush
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = pending.take() else {
            return;
        };
        if Instant::now() < *next_flush {
            *pending = Some(layout);
            return;
        }
        *next_flush = Instant::now() + inner.interval;
        (true, layout)
    };
    if !due {
        return;
    }
    let mut db = inner
        .db
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = db.as_mut() else {
        return; // fallback mode: the layout is deliberately forgotten
    };
    match write_layout(db, &layout) {
        Ok(()) => {
            inner.writes.fetch_add(1, Ordering::Relaxed);
        }
        Err(error) => {
            eprintln!("[session] failed to persist the layout: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use tiller_persistence::{AppearanceMode, FileIconTheme, ProjectRecord};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-session-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            // Resolve /var -> /private/var (macOS): the checkout walker
            // canonicalizes, so path comparisons must use canonical paths.
            let path = std::fs::canonicalize(&path).expect("canonicalize temp dir");
            Self(path)
        }

        fn db_path(&self, name: &str) -> PathBuf {
            self.0.join(format!("{name}.sqlite"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run_git(directory: &Path, arguments: &[&str]) {
        let status = std::process::Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .status()
            .expect("run git");
        assert!(status.success(), "git {:?} failed: {status}", arguments);
    }

    fn layout(path: &Path, tabs: Vec<SessionTab>) -> SessionLayout {
        SessionLayout {
            working_directory: path.to_path_buf(),
            branch: "main".to_string(),
            tabs,
            tab_states: Vec::new(),
        }
    }

    fn three_tabs() -> Vec<SessionTab> {
        vec![
            SessionTab {
                id: "chat".into(),
                title: "Chat".into(),
                kind: "chat".into(),
                agent_id: None,
                active: false,
            },
            SessionTab {
                id: "terminal-1".into(),
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: false,
            },
            SessionTab {
                id: "terminal-2".into(),
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            },
        ]
    }

    #[test]
    fn a_codex_agent_identity_round_trips_through_session_store() {
        let dir = TempDir::new();
        let db_path = dir.db_path("agent-identity-roundtrip");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");
        let tabs = vec![SessionTab {
            id: "codex".into(),
            title: "Codex".into(),
            kind: "chat".into(),
            active: true,
            agent_id: Some("codex".into()),
        }];

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, tabs));
        store.flush_now();

        let restored = restore(&db_path, Path::new("/nonexistent/fallback"));
        assert_eq!(restored.tabs[0].agent_id, Some("codex".into()));
    }

    #[test]
    fn a_layout_saves_and_restores_identically() {
        let dir = TempDir::new();
        let db_path = dir.db_path("roundtrip");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, three_tabs()));
        store.flush_now();
        assert_eq!(store.writes(), 1, "one write for one scheduled layout");

        let restored = restore(&db_path, Path::new("/nonexistent/fallback"));
        assert_eq!(
            restored.working_directory, working_directory,
            "the worktree directory comes back"
        );
        assert_eq!(
            restored.tabs,
            three_tabs(),
            "tabs, order and active flag come back"
        );
        assert!(
            restored.diagnostics.is_empty(),
            "a healthy database restores without diagnostics"
        );
    }

    #[test]
    fn pane_state_is_dumped_and_restored_with_the_tab() {
        let dir = TempDir::new();
        let db_path = dir.db_path("pane-state");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");
        let state = SessionTabState {
            root_id: Some(0),
            pane_events: vec![PaneEvent::Split {
                focused: 0,
                new_id: 1,
                direction: "horizontal".into(),
            }],
            scrollback: std::collections::BTreeMap::from([(0, b"P28_SCROLLBACK_NONCE".to_vec())]),
        };
        let layout = SessionLayout {
            working_directory: working_directory.clone(),
            branch: "main".into(),
            tabs: vec![SessionTab {
                id: "terminal".into(),
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![state.clone()],
        };

        let store = SessionStore::open(&db_path);
        store.schedule(layout);
        store.flush_now();
        let db = AppDatabase::open(&db_path).expect("reopen state database");
        let worktree_id = catalog_ids_for_path(&working_directory).2;
        let written = db
            .tab_states_of_worktree(&worktree_id)
            .expect("dump written tab state");
        assert_eq!(written.len(), 1);
        assert!(written[0].state.contains("horizontal"));

        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(restored.tab_states, vec![state]);
        assert_eq!(restored.tabs[0].title, "Terminal");
    }

    #[test]
    fn a_changes_tab_saves_and_restores_with_the_other_surfaces() {
        let dir = TempDir::new();
        let db_path = dir.db_path("changes-roundtrip");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");
        let mut tabs = three_tabs();
        tabs.insert(
            1,
            SessionTab {
                id: "changes".into(),
                title: "Changes".into(),
                kind: "diff".into(),
                agent_id: None,
                active: false,
            },
        );

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, tabs.clone()));
        store.flush_now();

        let restored = restore(&db_path, Path::new("/nonexistent/fallback"));
        assert_eq!(restored.tabs, tabs, "Changes remains a persisted peer tab");
    }

    #[test]
    fn a_browser_tab_saves_and_restores_with_the_other_surfaces() {
        let dir = TempDir::new();
        let db_path = dir.db_path("browser-roundtrip");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");
        let mut tabs = three_tabs();
        tabs.insert(
            1,
            SessionTab {
                id: "browser".into(),
                title: "Browser".into(),
                kind: "browser".into(),
                agent_id: None,
                active: false,
            },
        );

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, tabs.clone()));
        store.flush_now();

        let restored = restore(&db_path, Path::new("/nonexistent/fallback"));
        assert_eq!(restored.tabs, tabs, "Browser remains a persisted peer tab");
    }

    #[test]
    fn settings_round_trip_through_session_store_and_sqlite_rows() {
        let dir = TempDir::new();
        let db_path = dir.db_path("settings");
        let settings = AppSettings {
            appearance: AppearanceMode::Dark,
            ui_font_size: 17,
            terminal_font_size: 19,
            file_icon_theme: FileIconTheme::Material,
            control_socket_enabled: false,
            resume_agent_sessions: false,
            auto_naming: true,
            limit_chat_history: false,
            chat_retention: 37,
            limit_mounted_worktrees: true,
            mounted_worktrees: 17,
            summarizer_agent: "codex".into(),
            claude_show_in_bar: false,
            codex_show_in_bar: false,
            opencode_show_in_bar: true,
            refresh_interval_min: 11,
        };

        {
            let store = SessionStore::open(&db_path);
            store.save_settings(&settings);
            assert_eq!(store.load_settings(), settings);
        }

        let conn = rusqlite::Connection::open(&db_path).expect("open settings database");
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT key, value FROM setting ORDER BY key")
            .expect("prepare setting query")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query setting rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("read setting rows");
        assert_eq!(
            rows,
            vec![
                ("appearance.fileIconTheme".into(), "material".into()),
                ("appearance.terminalFontSize".into(), "19".into()),
                ("appearance.theme".into(), "dark".into()),
                ("appearance.uiFontSize".into(), "17".into()),
                ("chat.limitHistory".into(), "false".into()),
                ("chat.retentionCount".into(), "37".into()),
                ("controlSocket.enabled".into(), "false".into()),
                ("general.autoNaming".into(), "true".into()),
                ("general.summarizerAgent".into(), "codex".into()),
                ("session.resumeAgentSessions".into(), "false".into()),
                ("usage.claudeVisible".into(), "false".into()),
                ("usage.codexVisible".into(), "false".into()),
                ("usage.opencodeVisible".into(), "true".into()),
                ("usage.refreshIntervalMin".into(), "11".into()),
                ("worktrees.limitMounted".into(), "true".into()),
                ("worktrees.mountedCount".into(), "17".into()),
            ]
        );
        println!("sqlite setting rows: {rows:?}");

        let reopened = SessionStore::open(&db_path);
        assert_eq!(reopened.load_settings(), settings);
    }

    #[test]
    fn scrollback_is_bounded_at_the_persistence_seam() {
        let mut bytes = vec![b'a'; 17];
        bytes.extend(vec![b'x'; SCROLLBACK_LIMIT]);
        let bounded = SessionTabState::bounded_scrollback(&bytes);
        assert_eq!(bounded.len(), SCROLLBACK_LIMIT);
        assert!(bounded.iter().all(|byte| *byte == b'x'));
    }

    #[test]
    fn tab_state_lives_in_its_own_schema_table() {
        let dir = TempDir::new();
        let db_path = dir.db_path("tab-state-schema");
        let store = SessionStore::open(&db_path);
        store.schedule(layout(&dir.0, three_tabs()));
        store.flush_now();
        drop(store);

        let conn = rusqlite::Connection::open(&db_path).expect("open tab database");
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("prepare table query")
            .query_map([], |row| row.get(0))
            .expect("query tab schema")
            .collect::<Result<Vec<_>, _>>()
            .expect("read tables");
        assert!(tables.iter().any(|table| table == "tab_state"));
    }

    #[test]
    fn a_corrupt_database_falls_back_to_defaults() {
        let dir = TempDir::new();
        let db_path = dir.db_path("corrupt");
        std::fs::write(
            &db_path,
            "this is not a sqlite database, just enough bytes to not look empty........",
        )
        .expect("write garbage");

        // restore() must not error: it logs and returns the default layout.
        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(restored.working_directory, PathBuf::from("/tmp"));
        assert_eq!(restored.tabs.len(), 2, "default chat + terminal");
        assert!(
            restored
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("database unavailable")),
            "fallback restore must expose the corrupt-store diagnostic"
        );

        // The store must also survive in fallback mode: scheduling is safe
        // and produces no writes.
        let store = SessionStore::open(&db_path);
        assert!(!store.persistence_enabled());
        store.schedule(layout(Path::new("/tmp"), three_tabs()));
        store.flush_now();
        assert_eq!(store.writes(), 0, "fallback mode never writes");
    }

    #[test]
    fn a_newer_schema_database_is_refused_gracefully() {
        let dir = TempDir::new();
        let db_path = dir.db_path("newer");
        {
            let store = SessionStore::open(&db_path);
            store.schedule(layout(&dir.0, three_tabs()));
            store.flush_now();
        }
        // Simulate a newer app having bumped the schema version.
        {
            let conn = rusqlite::Connection::open(&db_path).expect("open raw");
            conn.pragma_update(None, "user_version", 999)
                .expect("bump version");
        }

        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(
            restored.working_directory,
            PathBuf::from("/tmp"),
            "a newer schema is refused, not half-read"
        );
        assert_eq!(restored.tabs.len(), 2);

        let store = SessionStore::open(&db_path);
        assert!(!store.persistence_enabled(), "newer schema disables writes");
    }

    #[test]
    fn the_debounce_collapses_a_burst_into_one_write() {
        let dir = TempDir::new();
        let db_path = dir.db_path("debounce");
        // A short interval so the test does not wait half a second per flush.
        let store = SessionStore::open_with(&db_path, Duration::from_millis(60));

        // Ten rapid changes, well inside one debounce window.
        for index in 0..10 {
            let mut tabs = three_tabs();
            tabs[0].title = format!("Chat {index}");
            store.schedule(layout(&dir.0, tabs));
        }
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            store.writes() <= 2,
            "a burst of 10 changes must not produce a write per change; wrote {}",
            store.writes()
        );

        // A later change flushes again once the window has passed.
        store.schedule(layout(&dir.0, three_tabs()));
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(
            store.writes(),
            2,
            "a quiet gap lets the next change flush on its own"
        );

        // And the last scheduled layout is the one that landed.
        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(restored.tabs, three_tabs());
    }

    #[test]
    fn a_layout_preserves_stable_tab_ids() {
        let dir = TempDir::new();
        let checkout = dir.0.join("checkout");
        std::fs::create_dir_all(&checkout).expect("checkout");
        let tabs = vec![SessionTab {
            id: "chat-stable".into(),
            title: "Chat".into(),
            kind: "chat".into(),
            agent_id: None,
            active: true,
        }];
        let store = SessionStore::open(&dir.db_path("stable-ids"));
        store.schedule(layout(&checkout, tabs.clone()));
        store.flush_now();
        assert_eq!(
            restore(&dir.db_path("stable-ids"), Path::new("/tmp")).tabs,
            tabs
        );
    }

    #[test]
    fn flush_now_writes_even_inside_the_debounce_window() {
        let dir = TempDir::new();
        let db_path = dir.db_path("flush-now");
        let store = SessionStore::open_with(&db_path, Duration::from_secs(60));

        store.schedule(layout(&dir.0, three_tabs()));
        assert_eq!(store.writes(), 0, "nothing written inside the window");
        store.flush_now();
        assert_eq!(store.writes(), 1, "flush_now bypasses the debounce");
    }

    #[test]
    fn flush_now_persists_a_new_snapshot_inside_a_later_debounce_window() {
        let dir = TempDir::new();
        let db_path = dir.db_path("flush-new-snapshot");
        let store = SessionStore::open_with(&db_path, Duration::from_secs(60));
        let first = layout(&dir.0, three_tabs());
        store.schedule(first);
        store.flush_now();

        let mut changed_tabs = three_tabs();
        changed_tabs[0].title = "Changed".into();
        store.schedule(layout(&dir.0, changed_tabs.clone()));
        store.flush_now();

        assert_eq!(
            store.writes(),
            2,
            "quit flush must not lose the latest state"
        );
        assert_eq!(
            restore(&db_path, Path::new("/tmp")).tabs[0].title,
            "Changed"
        );
    }

    #[test]
    fn a_missing_worktree_directory_falls_back_with_a_diagnostic() {
        let dir = TempDir::new();
        let db_path = dir.db_path("missing-dir");
        let store = SessionStore::open(&db_path);
        store.schedule(layout(&dir.0.join("gone-checkout"), three_tabs()));
        store.flush_now();

        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(
            restored.working_directory,
            PathBuf::from("/tmp"),
            "a checkout that vanished falls back instead of failing"
        );
        assert_eq!(restored.tabs.len(), 2);
    }

    #[test]
    fn project_catalog_discovers_git_and_non_git_directories_without_duplicates() {
        let dir = TempDir::new();
        let git_root = dir.0.join("repo");
        let plain_root = dir.0.join("notes");
        std::fs::create_dir_all(&git_root).expect("repo dir");
        std::fs::create_dir_all(&plain_root).expect("plain dir");
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&git_root)
            .status()
            .expect("git init");

        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&git_root).expect("discover git repo"));
        assert!(!catalog.add(&git_root).expect("deduplicate git repo"));
        assert!(catalog.add(&plain_root).expect("discover plain folder"));

        assert_eq!(catalog.projects().len(), 2);
        assert!(catalog.projects()[0].is_git);
        assert!(!catalog.projects()[0].worktrees.is_empty());
        assert!(!catalog.projects()[1].is_git);
        assert!(catalog.projects()[1].worktrees.is_empty());
    }

    /// F-SID-12 (catalog half): setting one worktree primary clears its
    /// project siblings, unsetting leaves none, and an unknown path is a
    /// named error — the transition the context menu drives. Primary is
    /// per-project: another project's marker is untouched.
    #[test]
    fn set_primary_flips_the_application_level_marker() {
        let dir = TempDir::new();
        let make_worktree = |branch: &str, primary: bool| CatalogWorktree {
            branch: branch.to_string(),
            path: dir.0.join(branch),
            is_primary: primary,
        };
        let project = |id: &str, worktrees: Vec<CatalogWorktree>| CatalogProject {
            id: id.to_string(),
            name: id.to_string(),
            root_path: dir.0.join(id),
            is_git: true,
            worktrees,
        };

        let mut catalog = ProjectCatalog::from_projects(vec![
            project(
                "alpha",
                vec![
                    make_worktree("alpha-main", true),
                    make_worktree("alpha-feat", false),
                ],
            ),
            project("beta", vec![make_worktree("beta-main", false)]),
        ]);

        // Set the sibling: alpha-main's marker clears, alpha-feat's comes on.
        let feat_path = dir.0.join("alpha-feat");
        catalog
            .set_primary(&feat_path, true)
            .expect("known worktree set");
        assert!(
            !catalog.projects()[0].worktrees[0].is_primary,
            "sibling cleared"
        );
        assert!(catalog.projects()[0].worktrees[1].is_primary);

        // Unset: the marker goes off, nothing else moves.
        catalog
            .set_primary(&feat_path, false)
            .expect("known worktree unset");
        assert!(!catalog.projects()[0].worktrees[0].is_primary);
        assert!(!catalog.projects()[0].worktrees[1].is_primary);

        // Set back on the original worktree.
        let main_path = dir.0.join("alpha-main");
        catalog
            .set_primary(&main_path, true)
            .expect("known worktree set");
        assert!(catalog.projects()[0].worktrees[0].is_primary);
        assert!(!catalog.projects()[0].worktrees[1].is_primary);

        // An unknown path is a named error, not a silent no-op.
        assert!(
            catalog
                .set_primary(&dir.0.join("not-a-worktree"), true)
                .is_err(),
            "a path outside the catalog must be rejected"
        );

        // Primary is per-project: beta's transition leaves alpha alone.
        catalog
            .set_primary(&dir.0.join("beta-main"), true)
            .expect("known other worktree set");
        assert!(
            catalog.projects()[0].worktrees[0].is_primary,
            "other project untouched"
        );
        assert!(catalog.projects()[1].worktrees[0].is_primary);
    }

    #[test]
    fn project_catalog_treats_bare_repositories_as_safe_projects() {
        let dir = TempDir::new();
        let bare_root = dir.0.join("repo.git");
        std::process::Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&bare_root)
            .status()
            .expect("git init --bare");

        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&bare_root).expect("discover bare repo"));
        assert_eq!(catalog.projects().len(), 1);
        assert!(catalog.projects()[0].worktrees.is_empty());
    }

    #[test]
    fn restoring_a_catalog_with_primary_and_linked_rows_keeps_one_project_identity() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let linked = dir.0.join("repo-linked");
        std::fs::create_dir_all(&primary).expect("repo dir");
        run_git(&primary, &["init", "--quiet"]);
        run_git(
            &primary,
            &["config", "user.email", "tiller-tests@example.com"],
        );
        run_git(&primary, &["config", "user.name", "Tiller Tests"]);
        std::fs::write(primary.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&primary, &["add", "README"]);
        run_git(&primary, &["commit", "--quiet", "-m", "fixture"]);
        let status = std::process::Command::new("git")
            .args(["worktree", "add", "--quiet", "-b", "linked"])
            .arg(&linked)
            .current_dir(&primary)
            .status()
            .expect("git worktree add");
        assert!(status.success(), "git worktree add failed: {status}");

        let database = dir.db_path("legacy-catalog");
        let db = AppDatabase::open(&database).expect("open database");
        let mut primary_record =
            ProjectRecord::new("legacy-primary", "repo", primary.to_string_lossy());
        primary_record.display_name = Some("A renamed repo".into());
        primary_record.color_hex = Some("green".into());
        primary_record.icon_kind = "icon".into();
        primary_record.icon_value = Some("git-branch".into());
        primary_record.order_idx = 0;
        db.save_project(&primary_record)
            .expect("save primary project");
        let mut linked_record =
            ProjectRecord::new("legacy-linked", "repo-linked", linked.to_string_lossy());
        linked_record.order_idx = 1;
        db.save_project(&linked_record)
            .expect("save linked project");
        drop(db);

        let restored = restore_catalog(&database);
        assert_eq!(restored.projects.len(), 1);
        assert_eq!(restored.projects[0].root_path, primary);
        assert_eq!(restored.projects[0].worktrees.len(), 2);
        let restored_settings = restored
            .settings
            .get(&restored.projects[0].id)
            .expect("project identity settings restore with canonical id");
        assert_eq!(
            restored_settings.display_name.as_deref(),
            Some("A renamed repo")
        );
        assert_eq!(restored_settings.color_hex.as_deref(), Some("green"));
        assert_eq!(restored_settings.icon_value.as_deref(), Some("git-branch"));

        let store = SessionStore::open(&database);
        store.schedule_catalog(&ProjectCatalog::from_restored(
            restored.projects.clone(),
            restored.settings.clone(),
        ));
        let normalized = AppDatabase::open(&database).expect("reopen normalized database");
        let projects = normalized.projects().expect("read normalized projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, restored.projects[0].id);
        let worktrees = normalized
            .worktrees_of_project(&projects[0].id)
            .expect("read normalized worktrees");
        assert_eq!(
            worktrees
                .iter()
                .map(|worktree| worktree.id.clone())
                .collect::<Vec<_>>(),
            vec![
                worktree_id(&projects[0].id, 0),
                worktree_id(&projects[0].id, 1),
            ]
        );
    }

    #[test]
    fn session_references_survive_store_reopen() {
        let dir = TempDir::new();
        let database = dir.db_path("session-refs");
        {
            let store = SessionStore::open(&database);
            store.save_session_ref("pane-nonce", "agent-session-nonce");
        }

        let reopened = SessionStore::open(&database);
        assert_eq!(
            reopened.load_session_refs().get("pane-nonce"),
            Some(&"agent-session-nonce".to_string())
        );
    }

    #[test]
    fn project_catalog_rejects_a_path_inside_an_existing_project() {
        let dir = TempDir::new();
        let root = dir.0.join("repo");
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).expect("nested directory");
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .expect("git init");

        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&root).expect("discover root"));
        assert!(!catalog.add(&nested).expect("deduplicate nested path"));
        assert_eq!(catalog.projects().len(), 1);
    }

    #[test]
    fn restore_skips_vanished_projects_and_keeps_surviving_projects() {
        let dir = TempDir::new();
        let surviving = dir.0.join("surviving");
        let vanished = dir.0.join("vanished");
        std::fs::create_dir_all(&surviving).expect("surviving dir");
        std::fs::create_dir_all(&vanished).expect("vanished dir");
        let db_path = dir.db_path("projects");
        let store = SessionStore::open(&db_path);
        store.schedule(SessionLayout {
            working_directory: surviving.clone(),
            branch: "main".into(),
            tabs: vec![],
            tab_states: vec![],
        });
        store.flush_now();
        let mut catalog = ProjectCatalog::default();
        catalog.add(&vanished).expect("discover vanished");
        catalog.add(&surviving).expect("discover surviving");
        store.schedule_catalog(&catalog);
        store.flush_now();
        std::fs::remove_dir_all(&vanished).expect("remove vanished directory");

        let restored = restore_catalog(&db_path);
        assert_eq!(restored.projects.len(), 1);
        assert_eq!(
            restored.projects[0].root_path,
            surviving.canonicalize().unwrap()
        );
        assert!(
            restored
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("vanished"))
        );
    }

    // ---- database path scoping -----------------------------------------

    /// Creates a fake git checkout (a directory containing a real `.git`).
    fn checkout(dir: &Path, name: &str) -> PathBuf {
        let root = dir.join(name);
        std::fs::create_dir_all(&root).expect("checkout dir");
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .expect("git init");
        root
    }

    /// Places a (fake) binary inside a checkout, as `target/debug/tiller`
    /// would be after a build.
    fn built_binary(checkout_root: &Path) -> PathBuf {
        let exe = checkout_root.join("target/debug/tiller");
        std::fs::create_dir_all(exe.parent().expect("exe parent")).expect("target dir");
        std::fs::write(&exe, b"").expect("exe file");
        exe
    }

    #[test]
    fn two_checkouts_resolve_to_different_database_paths() {
        // The regression pin for the shared-database footgun: two checkouts
        // of the app must never resolve to the same session database, no
        // matter how they are launched.
        let dir = TempDir::new();
        let checkout_a = checkout(&dir.0, "tiller-main");
        let checkout_b = checkout(&dir.0, "tiller-p57-dbscope");
        let exe_a = built_binary(&checkout_a);
        let exe_b = built_binary(&checkout_b);

        let path_a = database_path_for(&dir.0, &exe_a, &dir.0, true);
        let path_b = database_path_for(&dir.0, &exe_b, &dir.0, true);

        assert_ne!(
            path_a, path_b,
            "each checkout must own its own session database"
        );
        assert_eq!(
            path_a,
            database_path_for(&dir.0, &exe_a, &dir.0, true),
            "the mapping must be stable across calls"
        );
        assert!(
            path_a.starts_with(dir.0.join("checkouts"))
                && path_b.starts_with(dir.0.join("checkouts")),
            "scoped databases live under the app-support checkouts/ dir"
        );
    }

    #[test]
    fn the_scoped_path_is_human_readable_and_pinned_in_shape() {
        let dir = TempDir::new();
        let checkout_root = checkout(&dir.0, "tiller-rust-p57-dbscope");
        let exe = built_binary(&checkout_root);

        let path = database_path_for(&dir.0, &exe, &dir.0, true);

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("tiller.sqlite"),
            "the file name stays tiller.sqlite"
        );
        let scope_dir = path.parent().expect("scope dir");
        assert_eq!(
            scope_dir.parent(),
            Some(dir.0.join("checkouts").as_path()),
            "exactly one level under checkouts/"
        );
        let name = scope_dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("scope dir name");
        let Some(suffix) = name.strip_prefix("tiller-rust-p57-dbscope-") else {
            panic!("scope dir should read <basename>-<hash>, got {name:?}");
        };
        assert!(
            suffix.len() >= 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "the hash suffix is hex: {suffix:?}"
        );
    }

    #[test]
    fn an_exe_outside_any_checkout_keeps_the_stable_path() {
        // A shipped/installed binary (inside an .app bundle, in ~/.cargo/bin,
        // anywhere without a `.git` ancestor) must keep the user-wide path.
        let dir = TempDir::new();
        let installed = dir.0.join("Applications/Tiller.app/Contents/MacOS/tiller");
        std::fs::create_dir_all(installed.parent().expect("bundle dir")).expect("bundle");
        std::fs::write(&installed, b"").expect("binary");

        let path = database_path_for(&dir.0, &installed, &dir.0, true);
        assert_eq!(
            path,
            dir.0.join("tiller.sqlite"),
            "installed binaries keep the stable user database"
        );
    }

    #[test]
    fn the_cwd_fallback_scopes_shared_target_dir_builds() {
        // A build whose target dir lives outside the checkout (e.g. a
        // CARGO_TARGET_DIR shared across worktrees): the exe is not inside
        // any checkout, but the process CWD is. Debug builds use the CWD to
        // find the checkout.
        let dir = TempDir::new();
        let checkout_root = checkout(&dir.0, "tiller-main");
        let shared_exe = dir.0.join("shared-target/debug/tiller");
        std::fs::create_dir_all(shared_exe.parent().expect("target dir")).expect("target");
        std::fs::write(&shared_exe, b"").expect("exe file");

        let scoped = database_path_for(&dir.0, &shared_exe, &checkout_root, true);
        assert_ne!(scoped, dir.0.join("tiller.sqlite"));
        assert!(scoped.starts_with(dir.0.join("checkouts")));

        // Release builds (cwd_fallback off) must never be diverted by the
        // CWD: a shipped binary launched from inside a repo keeps the
        // stable path.
        let stable = database_path_for(&dir.0, &shared_exe, &checkout_root, false);
        assert_eq!(stable, dir.0.join("tiller.sqlite"));
    }

    #[test]
    fn a_git_file_worktree_is_detected_like_a_full_checkout() {
        // `git worktree` checkouts carry `.git` as a *file*, not a
        // directory; each worktree must still get its own database.
        let dir = TempDir::new();
        let worktree = dir.0.join("wt-feature");
        std::fs::create_dir_all(&worktree).expect("worktree dir");
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere\n").expect(".git file");
        let exe = built_binary(&worktree);

        let path = database_path_for(&dir.0, &exe, &dir.0, true);
        assert_ne!(
            path,
            dir.0.join("tiller.sqlite"),
            "a git worktree gets its own database"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_session_state_uses_xdg_state_home_and_documented_fallback() {
        let environment = std::collections::BTreeMap::from([
            ("HOME".to_string(), "/home/alice".to_string()),
            (
                "XDG_STATE_HOME".to_string(),
                "/run/user/1000/state".to_string(),
            ),
        ]);
        assert_eq!(
            app_support_root_for(&environment),
            PathBuf::from("/run/user/1000/state/TillerRust")
        );

        let fallback =
            std::collections::BTreeMap::from([("HOME".to_string(), "/home/alice".to_string())]);
        assert_eq!(
            app_support_root_for(&fallback),
            PathBuf::from("/home/alice/.local/state/TillerRust")
        );
    }
}

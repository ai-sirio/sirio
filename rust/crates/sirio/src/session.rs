//! Session persistence: the shell's layout saved as the user works, and
//! restored at launch.
//!
//! The shell is a single-worktree app: one working directory, one sidebar
//! project, one tab strip. What must survive a relaunch is that layout —
//! the project and worktree records (with stable project ids and worktree
//! ids preserved by path so launching in different directories never
//! clobbers each other's records), the open tabs in order with the active
//! one, and the sidebar selection. That is
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

use sirio_persistence::{
    AgentRef, AppDatabase, AppSettings, PersistenceError, ProjectRecord, SidebarState, TabRecord,
    TabStateRecord, WorktreeRecord, stable_worktree_id,
};
use sirio_project::{DiscoveredProject, discover_project, is_git_repository};

/// How long a burst of changes is held before one write. 500 ms is under the
/// reaction time between discrete user actions (a click then flushes at the
/// next quiet boundary) yet long enough that a burst of programmatic changes
/// collapses into a single write; `flush_now` on quit makes the window
/// lossless.
pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

/// Per-pane scrollback is bounded so a long-running terminal cannot make the
/// session database grow without limit. The renderer-owned capture seam is
/// still supplied by `sirio_terminal`; this is the persistence-side bound.
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
    /// F-CORE-WSP-08: a chat tab's unsent composer text, captured live from
    /// the `Chat` entity at save time (the same pattern `scrollback` above
    /// uses for a terminal tab) and pushed back through `Chat::control_compose`
    /// at restore, so a draft the user never hit Enter on survives a restart.
    #[serde(default)]
    pub chat_draft: String,
    /// #125 (spec R6.5): a browser tab's live address, captured at save time
    /// exactly the way `chat_draft` and `scrollback` above are, so a restored
    /// Browser tab comes back where the user left it rather than at the
    /// fallback page. Empty means nothing was captured -- a session written
    /// before this field existed, or a tab that never held a browser -- and
    /// restore then falls back on its own.
    #[serde(default)]
    pub browser_url: String,
    /// #323: an Editor tab's file, so the tab can come back pointing at the
    /// same document. Held here rather than derived because nothing else in
    /// the session record names a file. Empty means nothing was captured;
    /// restore then drops the tab, and so does a path that no longer
    /// resolves — see `restored_editor_path`.
    #[serde(default)]
    pub editor_path: String,
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
            chat_draft: self.chat_draft.clone(),
            browser_url: self.browser_url.clone(),
            editor_path: self.editor_path.clone(),
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
/// 1. `$SIRIO_DB` — explicit override (tests, demos, a power user).
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
/// app's "Application Support/Sirio" database: pointing the rewrite at it opened
/// a 21 MB production file, ran its own migrations inside it, and left two
/// foreign tables behind. The rewrite keeps its own state until it replaces
/// the Swift app outright. On Linux the stable root is the XDG state directory.
pub fn database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SIRIO_DB") {
        return PathBuf::from(path);
    }
    let root = app_support_root();
    migrate_legacy_state_root(&root);
    let exe = std::env::current_exe().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let path = database_path_for(&root, &exe, &cwd, cfg!(debug_assertions));
    if path != root.join("sirio.sqlite") {
        eprintln!(
            "[session] development build: session state is checkout-local at {}",
            path.display()
        );
    }
    path
}

/// #125 (spec R6.1): the browser profile directory, scoped exactly the way the
/// session database is.
///
/// Derived from [`database_path`]'s own directory rather than re-deriving the
/// rule, so the three cases it already handles -- the `SIRIO_DB` override, the
/// checkout containing the running binary, the stable installed location --
/// hold here by construction and cannot drift apart later.
///
/// Sibling of the database rather than inside it: WebView2 owns everything
/// under its user-data folder and will happily write locks and caches there.
pub fn browser_profile_path() -> PathBuf {
    browser_profile_path_for(&database_path())
}

/// The profile directory beside one database path. Split out so the rule is
/// testable without touching the process environment.
fn browser_profile_path_for(database: &Path) -> PathBuf {
    database
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("browser")
}

/// The root under which every Sirio database lives: the stable
/// `sirio.sqlite` for installed binaries plus a `checkouts/` subtree with
/// one directory per development checkout. Linux uses XDG_STATE_HOME (or
/// `$HOME/.local/state`); Windows uses LOCALAPPDATA (it has no XDG layout);
/// macOS retains Application Support.
/// Moves a pre-rebrand `TillerRust` state directory to its new name, once.
///
/// Every session, chat transcript and browser profile the app has ever written
/// lives under this root. Starting fresh at the new path would present as
/// total data loss, so the directory is renamed rather than recreated. The
/// rename is atomic, and a no-op on every later launch because the new root
/// exists by then.
fn migrate_legacy_state_root(root: &Path) {
    if root.exists() {
        return;
    }
    let Some(parent) = root.parent() else {
        return;
    };
    let legacy = parent.join("TillerRust");
    if !legacy.exists() {
        return;
    }
    if let Err(error) = std::fs::rename(&legacy, root) {
        eprintln!(
            "[session] could not migrate {} to {}: {error}",
            legacy.display(),
            root.display()
        );
    }
}

fn app_support_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/Sirio"))
            .unwrap_or_else(|| std::env::temp_dir().join("Sirio"))
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
        environment
            .get("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/Sirio"))
            .unwrap_or_else(|| std::env::temp_dir().join("Sirio"))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let state_home = environment
            .get("XDG_STATE_HOME")
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    // Windows has no HOME and no XDG layout: LOCALAPPDATA
                    // is the non-roaming state root (the closest counterpart
                    // to XDG_STATE_HOME), so a native launch never lands in
                    // %TEMP%, where session state is swept away by OS
                    // cleanup. Temp stays the last resort when even the
                    // profile variables are absent — the same degrade the
                    // unix branch already used.
                    environment
                        .get("LOCALAPPDATA")
                        .map(Path::new)
                        .filter(|path| path.is_absolute())
                        .map(Path::to_path_buf)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    environment
                        .get("HOME")
                        .map(Path::new)
                        .filter(|path| path.is_absolute())
                        .map(|home| home.join(".local/state"))
                }
            })
            .unwrap_or_else(std::env::temp_dir);
        state_home.join("Sirio")
    }
}

/// Resolves where this process's session database lives.
///
/// - `$SIRIO_DB` always wins and is used verbatim (handled by the caller).
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
    app_support_root.join("sirio.sqlite")
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
/// `checkouts/<basename>-<hash>/sirio.sqlite` under the app-support root.
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
        .join("sirio.sqlite")
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
    /// Stable agent identity for an agent-backed tab, qualified as
    /// `adapter:<id>`/`registry:<id>` (see [`AgentRef`]) since v15 of the
    /// schema. Restore resolves it against the adapter catalog via
    /// [`AgentRef::adapter_id`]; a value that does not resolve restores as
    /// unresolvable, the same path as `None`.
    pub agent_id: Option<AgentRef>,
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
    /// F-PRJ-17: the branch/ref new worktrees for this project are created
    /// from by default (e.g. the current branch, a pinned branch, or the
    /// primary worktree), when the user has not typed one explicitly in the
    /// New Worktree prompt. Threaded through `write_catalog`/`restore_catalog`
    /// to the already-migrated `project.default_worktree_base` column.
    pub default_worktree_base: Option<String>,
    /// F-PRJ-18: overrides `resolve_parent_directory`'s default (a sibling
    /// of the primary worktree) with a fixed parent directory every new
    /// worktree for this project is created under. Threaded the same way,
    /// to `project.worktree_location_override`.
    pub worktree_location_override: Option<String>,
}

impl Default for CatalogProjectSettings {
    fn default() -> Self {
        Self {
            color_hex: None,
            display_name: None,
            icon_kind: "icon".to_string(),
            icon_value: None,
            default_worktree_base: None,
            worktree_location_override: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectCatalog {
    projects: Vec<CatalogProject>,
    settings: BTreeMap<String, CatalogProjectSettings>,
    /// Worktrees retained after Git removed them while a terminal was still
    /// mounted. The path remains in `projects` until that terminal is gone,
    /// but is no longer treated as a selectable checkout by the sidebar.
    missing_worktrees: HashSet<PathBuf>,
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
        Self {
            projects,
            settings,
            missing_worktrees: HashSet::new(),
        }
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

    pub fn is_worktree_missing(&self, path: &Path) -> bool {
        self.missing_worktrees.contains(&canonical_path(path))
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
        self.missing_worktrees.retain(|path| {
            self.projects.iter().any(|project| {
                project
                    .worktrees
                    .iter()
                    .any(|worktree| canonical_path(&worktree.path) == *path)
            })
        });
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
        let removed = self.projects.remove(index);
        for worktree in removed.worktrees {
            self.missing_worktrees
                .remove(&canonical_path(&worktree.path));
        }
        self.settings.remove(id);
        true
    }

    /// Re-discovers one project after an external repository transition such
    /// as `git init`, preserving its stable catalog identity.
    pub fn refresh_project(&mut self, id: &str) -> Result<(), String> {
        self.refresh_project_with_mounted_worktrees(id, &[])
    }

    /// Re-discovers one project while retaining Git rows whose terminals are
    /// still mounted. Git no longer reports those paths after an external
    /// `worktree remove`, but dropping the rows would orphan the live
    /// terminal entities. They are kept in the catalog and marked missing;
    /// ordinary refreshes without mounted paths drop them completely.
    pub fn refresh_project_with_mounted_worktrees(
        &mut self,
        id: &str,
        mounted_paths: &[PathBuf],
    ) -> Result<(), String> {
        let index = self
            .projects
            .iter()
            .position(|project| project.id == id)
            .ok_or_else(|| format!("unknown project: {id}"))?;
        let root = self.projects[index].root_path.clone();
        let previous = self.projects[index].worktrees.clone();
        let discovered = discover_project(&root).map_err(|error| error.to_string())?;
        let mut replacement = catalog_project(&root, discovered);
        replacement.id = id.to_string();

        let discovered_paths: HashSet<PathBuf> = replacement
            .worktrees
            .iter()
            .map(|worktree| canonical_path(&worktree.path))
            .collect();
        let mounted_paths: HashSet<PathBuf> = mounted_paths
            .iter()
            .map(|path| canonical_path(path))
            .collect();
        let mut missing_worktrees = std::mem::take(&mut self.missing_worktrees);
        for worktree in &previous {
            missing_worktrees.remove(&canonical_path(&worktree.path));
        }
        for worktree in &mut replacement.worktrees {
            if let Some(previous) = previous.iter().find(|previous| {
                canonical_path(&previous.path) == canonical_path(&worktree.path)
            }) {
                worktree.is_primary = previous.is_primary;
            }
            missing_worktrees.remove(&canonical_path(&worktree.path));
        }
        for worktree in previous {
            let path = canonical_path(&worktree.path);
            if !discovered_paths.contains(&path) && mounted_paths.contains(&path) {
                missing_worktrees.insert(path);
                replacement.worktrees.push(worktree);
            }
        }
        self.projects[index] = replacement;
        self.missing_worktrees = missing_worktrees;
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
    // F-SID-11: a plain (non-git) folder project has no git worktrees to
    // discover, so `discovered.worktrees` is empty and the sidebar had no
    // row at all to display branch/path/Primary/status/comment for it --
    // unlike a git project, which always has at least its primary
    // worktree. Synthesize one so a folder project gets the same baseline
    // row a git project's primary worktree gets.
    //
    // A bare repository (`git init --bare`) also has `is_git: false` here
    // (`is_git_repository` only checks for a nested `.git` entry, which a
    // bare repo's root doesn't have) and an empty worktree list, but its
    // root is not a usable checkout -- synthesizing a "primary worktree"
    // pointing at it would tell the sidebar to treat the bare repo's
    // internal object store as a working directory. `HEAD` + `objects/` at
    // the root is the cheap, no-shell-out signature every bare repo has
    // (and a plain folder essentially never does), so it is excluded.
    let looks_like_bare_git_repo =
        root_path.join("HEAD").is_file() && root_path.join("objects").is_dir();
    let worktrees =
        if !discovered.is_git && discovered.worktrees.is_empty() && !looks_like_bare_git_repo {
            vec![CatalogWorktree {
                branch: String::new(),
                path: root_path.to_path_buf(),
                is_primary: true,
            }]
        } else {
            discovered
                .worktrees
                .into_iter()
                // `git worktree list --porcelain` keeps reporting a worktree
                // after its checkout directory is deleted outside Sirio,
                // marked `prunable` -- it is not available locally, so it
                // must not become a sidebar row (a still-mounted terminal
                // for it is separately kept alive and labelled "(missing)"
                // by `refresh_project_with_mounted_worktrees`).
                .filter(|worktree| !worktree.prunable)
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
                .collect()
        };
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

/// Stable project identity and the initial Git-derived worktree identity for
/// the shell's single project and worktree. Once a worktree is persisted,
/// [`SessionStore::persisted_worktree_id`] preserves its id by path even if
/// Git changes the worktree list order.
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

fn worktree_id(project_id: &str, path: &Path) -> String {
    stable_worktree_id(project_id, path)
}

fn path_derived_catalog_ids(working_directory: &Path) -> (PathBuf, String, String) {
    let working_directory = canonical_path(working_directory);
    let project_id = project_id(&working_directory);
    (
        working_directory.clone(),
        project_id.clone(),
        worktree_id(&project_id, &working_directory),
    )
}

fn catalog_ids_for_discovered_path(
    working_directory: &Path,
    discovered: &DiscoveredProject,
) -> (PathBuf, String, String) {
    if !discovered.is_git || discovered.worktrees.is_empty() {
        return path_derived_catalog_ids(working_directory);
    }

    let working_directory = canonical_path(working_directory);
    let root = catalog_root(&working_directory, discovered);
    let project_id = project_id(&root);
    let Some(_index) = discovered
        .worktrees
        .iter()
        .position(|worktree| canonical_path(&worktree.path) == working_directory)
    else {
        return path_derived_catalog_ids(&working_directory);
    };
    (root, project_id.clone(), worktree_id(&project_id, &working_directory))
}

/// Resolves a worktree through Git when no persisted catalog row identifies
/// its path. The persisted catalog is deliberately not consulted here: this
/// is the fallback used for a path the user has not added to the catalog yet.
fn catalog_ids_for_path(working_directory: &Path) -> (PathBuf, String, String) {
    let working_directory = canonical_path(working_directory);
    if let Ok(discovered) = discover_project(&working_directory) {
        return catalog_ids_for_discovered_path(&working_directory, &discovered);
    }
    path_derived_catalog_ids(&working_directory)
}

/// Resolves the stable persisted identity shared by layout readers and
/// writers. A catalog row matched by path wins over Git's current worktree
/// index, which may change when another worktree is removed or reordered.
fn worktree_id_for_database(
    db: &AppDatabase,
    working_directory: &Path,
) -> Result<Option<String>, PersistenceError> {
    if let Some((_, _, worktree_id)) = persisted_catalog_ids_for_path(db, working_directory)? {
        return Ok(Some(worktree_id));
    }
    if !working_directory.is_dir() {
        return Ok(None);
    }
    Ok(Some(catalog_ids_for_path(working_directory).2))
}

/// Resolves a persisted worktree identity using a database path. This is the
/// bridge for the standalone restore builders, which do not own a
/// [`SessionStore`] handle. A database/open or query error leaves the same
/// Git-derived fallback available as before.
pub fn persisted_worktree_id_for_database(database: &Path, working_directory: &Path) -> String {
    match AppDatabase::open(database) {
        Ok(db) => match worktree_id_for_database(&db, working_directory) {
            Ok(Some(worktree_id)) => worktree_id,
            Ok(None) => catalog_ids_for_path(working_directory).2,
            Err(error) => {
                eprintln!(
                    "[session] failed to resolve worktree identity for {}: {error}; using Git fallback",
                    working_directory.display()
                );
                catalog_ids_for_path(working_directory).2
            }
        },
        Err(error) => {
            eprintln!(
                "[session] failed to open {} for worktree identity: {error}; using Git fallback",
                database.display()
            );
            catalog_ids_for_path(working_directory).2
        }
    }
}

/// Writes one layout to the database: the project and worktree records
/// (upserted), the worktree's tabs (replaced atomically, with the active
/// flag normalized), and the sidebar selection (read-modify-write so other
/// projects' expansion state survives).
fn write_layout(db: &AppDatabase, layout: &SessionLayout) -> Result<(), PersistenceError> {
    let ids_from_database = persisted_catalog_ids_for_path(db, &layout.working_directory)?;
    let (project_root, project_id, worktree_id) = if let Some(ids) = ids_from_database {
        ids
    } else if !layout.working_directory.is_dir() {
        eprintln!(
            "[session] dropping layout for unresolved missing worktree: {}",
            layout.working_directory.display()
        );
        return Ok(());
    } else if is_git_repository(&layout.working_directory) {
        eprintln!(
            "[session] dropping layout for unresolved git worktree: {}",
            layout.working_directory.display()
        );
        return Ok(());
    } else {
        catalog_ids_for_path(&layout.working_directory)
    };
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

/// Allocates a new tab identity from a database-backed worktree identity.
/// The timestamp prevents reuse after a tab is closed while the counter keeps
/// same-millisecond allocations distinct.
fn new_tab_id_for_worktree(worktree_id: &str, counter: usize) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{worktree_id}-tab-{timestamp:x}-{counter}")
}

/// Resolves a layout path through the durable catalog before asking Git.
/// Linked worktrees can retain a `.git` file after the main repository has
/// moved; Git cannot resolve those paths, but the persisted worktree row still
/// identifies the project that owns them. Returning that identity prevents a
/// failed discovery from being turned into a new top-level project.
fn persisted_catalog_ids_for_path(
    db: &AppDatabase,
    working_directory: &Path,
) -> Result<Option<(PathBuf, String, String)>, PersistenceError> {
    let requested_path = working_directory.to_string_lossy();
    let canonical_working_directory = working_directory.canonicalize().ok();
    let Some(worktree) = db
        .worktrees()?
        .into_iter()
        .find(|worktree| {
            worktree.path == requested_path
                || canonical_working_directory.as_ref().is_some_and(|canonical| {
                    Path::new(&worktree.path).canonicalize().ok().as_ref() == Some(canonical)
                })
        })
    else {
        return Ok(None);
    };
    let Some(project) = db
        .projects()?
        .into_iter()
        .find(|project| project.id == worktree.project_id)
    else {
        return Ok(None);
    };
    Ok(Some((
        canonical_path(Path::new(&project.root_path)),
        worktree.project_id,
        worktree.id,
    )))
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
        record.default_worktree_base = settings.default_worktree_base;
        record.worktree_location_override = settings.worktree_location_override;
        db.save_project(&record)?;

        // F-CTRL-WORK-01: this upsert re-derives every worktree row from
        // the in-memory catalog, which carries no `comment` field. Without
        // carrying the existing row's comment/created_at forward, every
        // `schedule_catalog` call (startup normalization, project add,
        // clone, ...) would blow away a comment set via `worktree.set`,
        // even though that write went through `persist_worktree_comment`
        // moments earlier.
        let existing_by_id: std::collections::HashMap<String, WorktreeRecord> = db
            .worktrees_of_project(&project.id)?
            .into_iter()
            .map(|worktree| (worktree.id.clone(), worktree))
            .collect();
        let mut existing_by_path = std::collections::HashMap::new();
        for worktree in existing_by_id.values() {
            existing_by_path
                .entry(canonical_path(Path::new(&worktree.path)))
                .or_insert_with(|| worktree.id.clone());
        }
        let mut worktree_ids = Vec::with_capacity(project.worktrees.len());
        let mut desired_worktree_ids = HashSet::new();
        for worktree in &project.worktrees {
            let id = existing_by_path
                .get(&canonical_path(&worktree.path))
                .cloned()
                .unwrap_or_else(|| worktree_id(&project.id, &worktree.path));
            desired_worktree_ids.insert(id.clone());
            worktree_ids.push(id);
        }
        if project.is_git {
            for worktree in existing_by_id.values() {
                if !desired_worktree_ids.contains(&worktree.id) {
                    db.remove_worktree(&worktree.id)?;
                }
            }
        }
        for (worktree_index, (worktree, id)) in project
            .worktrees
            .iter()
            .zip(worktree_ids)
            .enumerate()
        {
            let mut record = WorktreeRecord::new(
                id.clone(),
                &project.id,
                &worktree.branch,
                worktree.path.to_string_lossy(),
            );
            record.order_idx = worktree_index as i64;
            record.is_primary = worktree.is_primary;
            if let Some(existing) = existing_by_id.get(&id) {
                record.comment = existing.comment.clone();
                record.created_at = existing.created_at;
                // #323: same hazard as the comment above -- this upsert
                // re-derives the row from a catalog that has no idea a pane
                // is open, so without carrying it forward every startup
                // normalization would close it.
                record.secondary_pane_open = existing.secondary_pane_open;
            }
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
    let project_records = db.projects().unwrap_or_default();
    let persisted_worktrees = db.worktrees().unwrap_or_default();
    let persisted_project_ids: HashSet<String> = project_records
        .iter()
        .map(|project| project.id.clone())
        .collect();
    for record in project_records {
        let root = PathBuf::from(&record.root_path);
        if !root.is_dir() {
            diagnostics.push(format!(
                "project {} vanished: {}",
                record.name,
                root.display()
            ));
            continue;
        }
        if persisted_worktrees.iter().any(|worktree| {
            worktree.project_id != record.id
                && persisted_project_ids.contains(worktree.project_id.as_str())
                && canonical_path(Path::new(&worktree.path)) == canonical_path(&root)
        }) {
            diagnostics.push(format!(
                "project {} is a linked worktree of another project and was dropped",
                record.name
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
                            default_worktree_base: record.default_worktree_base.clone(),
                            worktree_location_override: record.worktree_location_override.clone(),
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
            Err(error) => {
                // F-CHG-02/03/09: a transient git fault (e.g. `.git`
                // briefly unreadable) must not make a durable project the
                // user added silently disappear. Dropping it here used to
                // do three things at once: erase the sidebar row, desync
                // it from the working directory/right panel that restore()
                // (a separate, catalog-independent read of the same
                // persisted path) still pointed at, and -- since
                // `write_catalog` deletes any DB row missing from the next
                // `schedule_catalog` call -- risk turning one bad `git`
                // invocation into a permanent loss on the next unrelated
                // catalog write. Keep the project with its last persisted
                // worktrees instead: the Files/Changes surfaces' own 1s
                // refresh loops already retry the real git calls and show
                // their own "unavailable" + Retry state, and self-heal
                // once the fault clears -- they just need the project to
                // still be there to render into.
                let worktrees = db.worktrees_of_project(&record.id).unwrap_or_default();
                let project = degraded_catalog_project(&record, worktrees);
                if seen_project_ids.insert(project.id.clone()) {
                    settings.insert(
                        project.id.clone(),
                        CatalogProjectSettings {
                            color_hex: record.color_hex.clone(),
                            display_name: record.display_name.clone(),
                            icon_kind: record.icon_kind.clone(),
                            icon_value: record.icon_value.clone(),
                            default_worktree_base: record.default_worktree_base.clone(),
                            worktree_location_override: record.worktree_location_override.clone(),
                        },
                    );
                    projects.push(project);
                }
                diagnostics.push(format!(
                    "project {} could not be refreshed, keeping its last-known state: {error}",
                    record.name
                ));
            }
        }
    }
    RestoredCatalog {
        projects,
        settings,
        diagnostics,
    }
}

/// Builds a [`CatalogProject`] straight from what was last persisted, for
/// when a fresh `discover_project` failed (see the call site in
/// [`restore_catalog`]). Unlike [`catalog_project`], this never shells out
/// to git -- it is exactly the durable state the user last saw, not a
/// rediscovery.
fn degraded_catalog_project(
    record: &ProjectRecord,
    worktrees: Vec<WorktreeRecord>,
) -> CatalogProject {
    let root_path = PathBuf::from(&record.root_path);
    let mut worktrees: Vec<CatalogWorktree> = worktrees
        .into_iter()
        .map(|worktree| CatalogWorktree {
            branch: worktree.branch,
            path: PathBuf::from(worktree.path),
            is_primary: worktree.is_primary,
        })
        .collect();
    if worktrees.is_empty() {
        // No worktree row was ever persisted for this project (unusual,
        // but not impossible for an older database) -- fall back to a
        // synthesized primary at the root, the same baseline row a
        // freshly discovered git project always gets at least one of.
        worktrees.push(CatalogWorktree {
            branch: "main".to_string(),
            path: root_path.clone(),
            is_primary: true,
        });
    }
    CatalogProject {
        id: record.id.clone(),
        name: record.name.clone(),
        root_path,
        // A worktree row only ever gets persisted for a git project
        // (`write_catalog` only writes worktrees `if project.is_git`), so
        // reaching this function at all means the last-known state was a
        // git repository.
        is_git: true,
        worktrees,
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

    match tabs_for_worktree(db, &worktree.id, working_directory) {
        Ok(restored) => restored,
        Err(error) => {
            eprintln!("[session] failed to read tabs: {error}; using the default layout");
            default_restored(fallback_directory)
        }
    }
}

/// The shared tail of [`restore_from`] and [`SessionStore::restore_tabs_for`]:
/// given a worktree's own stable id (see
/// [`SessionStore::persisted_worktree_id`]) and the directory it lives at,
/// reads its persisted tabs and their pane state.
/// Factored out so a runtime worktree switch (CENTER-01) can load a
/// worktree's own tabs the same way boot already does, rather than only
/// ever reading whichever worktree the database happens to remember as
/// last-selected.
fn tabs_for_worktree(
    db: &AppDatabase,
    worktree_id: &str,
    working_directory: PathBuf,
) -> Result<RestoredSession, PersistenceError> {
    let records = db.tabs_of_worktree(worktree_id)?;

    let mut diagnostics = Vec::new();
    let state_records = match db.tab_states_of_worktree(worktree_id) {
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

    Ok(RestoredSession {
        working_directory,
        tabs,
        tab_states,
        diagnostics,
    })
}

fn default_restored(fallback_directory: &Path) -> RestoredSession {
    RestoredSession {
        working_directory: fallback_directory.to_path_buf(),
        tabs: SessionLayout::default_in(fallback_directory.to_path_buf()).tabs,
        tab_states: SessionLayout::default_in(fallback_directory.to_path_buf()).tab_states,
        diagnostics: Vec::new(),
    }
}

/// The layout for a worktree whose directory is gone: no tabs at all.
///
/// Deliberately not [`default_restored`]. A directory that does not exist
/// cannot host a shell, so handing back the default Chat and Terminal strip
/// only builds a pane whose spawn fails — and a failed pane reports `Error`,
/// which `requires_close_confirmation` reads as live work worth protecting,
/// leaving the centre pane bolted to a worktree that is not there any more.
/// [`write_layout`] already refuses to persist a layout for exactly this
/// case; this is the read side agreeing with the write side.
fn empty_restored(fallback_directory: &Path) -> RestoredSession {
    RestoredSession {
        working_directory: fallback_directory.to_path_buf(),
        tabs: Vec::new(),
        tab_states: Vec::new(),
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
    /// Where [`SessionStore::open`] was pointed. Kept so a caller holding a
    /// store never has to re-derive it through the process-global
    /// [`database_path`] -- which answers for the *app*, not for this store,
    /// and is a different database in every test.
    path: PathBuf,
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
                path: path.to_path_buf(),
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

    /// The database this store was opened on.
    pub fn database_path(&self) -> &Path {
        &self.inner.path
    }

    /// Returns the worktree identity used by persisted tabs and chats. A
    /// path already present in the durable catalog keeps its row id even
    /// when Git reports the worktree at a different list index.
    pub fn persisted_worktree_id(&self, working_directory: &Path) -> String {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return catalog_ids_for_path(working_directory).2;
        };
        match worktree_id_for_database(db, working_directory) {
            Ok(Some(worktree_id)) => worktree_id,
            Ok(None) => catalog_ids_for_path(working_directory).2,
            Err(error) => {
                eprintln!(
                    "[session] failed to resolve worktree identity for {}: {error}; using Git fallback",
                    working_directory.display()
                );
                catalog_ids_for_path(working_directory).2
            }
        }
    }

    /// Allocates a new tab identity under this store's database-backed
    /// worktree identity.
    pub fn new_tab_id(&self, working_directory: &Path, counter: usize) -> String {
        new_tab_id_for_worktree(&self.persisted_worktree_id(working_directory), counter)
    }

    /// Loads `working_directory`'s own persisted tabs from *this store's*
    /// database (CENTER-01), independent of whatever the database currently
    /// remembers as the last-selected worktree. `select_worktree` (main.rs)
    /// calls this on every runtime worktree switch so the centre pane
    /// mounts the *newly selected* worktree's own tabs rather than leaving
    /// the previous selection's tabs mounted and simply relabelled -- the
    /// stale-centre-pane defect. A worktree with nothing persisted yet (or
    /// a database that never opened) yields the same empty-but-valid layout
    /// `default_restored` gives a first-ever launch. Reuses the connection
    /// `self` already holds open rather than opening a second one at the
    /// app-wide [`database_path`] -- the distinction matters for a test
    /// that opens its own isolated database (as several in `main.rs` do),
    /// where reaching for `database_path()` here would silently read the
    /// wrong file.
    ///
    /// A directory that is *gone* is the one case that yields no tabs at
    /// all rather than the default strip -- see [`empty_restored`] for why
    /// handing back a Chat and a Terminal there is worse than handing back
    /// nothing.
    pub fn restore_tabs_for(&self, working_directory: &Path) -> RestoredSession {
        if !working_directory.is_dir() {
            return empty_restored(working_directory);
        }
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return default_restored(working_directory);
        };
        let Some(worktree_id) = (match worktree_id_for_database(db, working_directory) {
            Ok(worktree_id) => worktree_id,
            Err(error) => {
                eprintln!(
                    "[session] failed to resolve worktree identity for {}: {error}; using an empty layout",
                    working_directory.display()
                );
                None
            }
        }) else {
            return default_restored(working_directory);
        };
        match tabs_for_worktree(db, &worktree_id, working_directory.to_path_buf()) {
            Ok(restored) => restored,
            Err(error) => {
                eprintln!(
                    "[session] failed to read tabs for {worktree_id}: {error}; using an empty layout"
                );
                default_restored(working_directory)
            }
        }
    }

    /// The tabs actually persisted for a worktree, in strip order, and
    /// nothing else: empty for a worktree with no row, no tabs, or a database
    /// in fallback mode. Unlike [`Self::restore_tabs_for`] this never
    /// invents the default Chat + Terminal strip — it answers "what does this
    /// worktree hold?", for the sidebar's parked rows, where a worktree the
    /// user never opened must show nothing rather than a strip that would
    /// only come into being on selection.
    pub fn persisted_tabs_for(&self, working_directory: &Path) -> Vec<SessionTab> {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return Vec::new();
        };
        let worktree_id = match worktree_id_for_database(db, working_directory) {
            Ok(Some(worktree_id)) => worktree_id,
            Ok(None) => return Vec::new(),
            Err(error) => {
                eprintln!(
                    "[session] failed to resolve worktree identity for {}: {error}; listing no tabs",
                    working_directory.display()
                );
                return Vec::new();
            }
        };
        match tabs_for_worktree(db, &worktree_id, working_directory.to_path_buf()) {
            Ok(restored) => restored.tabs,
            Err(error) => {
                eprintln!(
                    "[session] failed to read tabs for {worktree_id}: {error}; listing no tabs"
                );
                Vec::new()
            }
        }
    }

    /// #323: whether this worktree's Secondary centre pane was open when the
    /// session was last written. `false` for a worktree with no row yet, and
    /// for a database in fallback mode -- a closed pane is the safe answer,
    /// since it is also what a worktree with no Secondary tabs shows.
    pub fn secondary_pane_open_for(&self, working_directory: &Path) -> bool {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return false;
        };
        match db.worktree_by_path(&working_directory.to_string_lossy()) {
            Ok(record) => record.is_some_and(|record| record.secondary_pane_open),
            Err(error) => {
                eprintln!("[session] failed to read the pane flag: {error}; assuming closed");
                false
            }
        }
    }

    /// #323: records that this worktree's Secondary pane is open or closed.
    /// A no-op for a worktree with no persisted row -- the row is written by
    /// the catalog upsert, and a pane state with no worktree to hang off is
    /// not worth inventing one for.
    pub fn save_secondary_pane_open(&self, working_directory: &Path, open: bool) {
        let db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_ref() else {
            return;
        };
        let path = working_directory.to_string_lossy();
        match db.worktree_by_path(&path) {
            Ok(Some(mut record)) => {
                if record.secondary_pane_open == open {
                    return;
                }
                record.secondary_pane_open = open;
                if let Err(error) = db.save_worktree(&record) {
                    eprintln!("[session] failed to persist the pane flag for {path}: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("[session] failed to read the worktree row for {path}: {error}");
            }
        }
    }

    /// Persists `layout` immediately, bypassing the debounce. CENTER-01: a
    /// runtime worktree switch that reloads `self.tabs` for the newly
    /// selected worktree drops the outgoing worktree's in-memory state at
    /// the same moment, and the regular debounced [`Self::schedule`] write
    /// that follows the switch always captures whichever `working_directory`
    /// is current *by the time it flushes* -- i.e. the new one. Anything
    /// from the outgoing worktree not yet flushed (an unsent chat draft, a
    /// title not yet auto-named) would otherwise be silently lost rather
    /// than merely delayed, so the switch calls this first, synchronously,
    /// for the worktree it is about to leave.
    pub fn save_layout_now(&self, layout: &SessionLayout) {
        let _perf = sirio_perf::span("SessionStore.save_layout_now", 0);
        let mut db = self
            .inner
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(db) = db.as_mut() else {
            return;
        };
        match write_layout(db, layout) {
            Ok(()) => {
                self.inner.writes.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                eprintln!("[session] failed to save the outgoing worktree before switch: {error}");
            }
        }
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
        let _perf = sirio_perf::span("SessionStore.save_settings", 0);
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
        let _perf = sirio_perf::span("SessionStore.flush_now", 0);
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
    use sirio_persistence::{AppearanceMode, BaseColor, ProjectRecord};
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-session-test-{}-{unique}",
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

    /// The spelling git accepts for a fixture path on its command line.
    ///
    /// [`TempDir`] canonicalizes, which on Windows yields a *verbatim*
    /// `\\?\C:\...` path. Git does not understand that prefix and refuses
    /// the argument outright (`could not create leading directories of
    /// '//?/C:/...': Invalid argument`), so every fixture below that hands
    /// a path straight to `git` converts it here first. Production code has
    /// the same conversion built in (`sirio_git::git::path_arg`), which is
    /// why only the fixtures are affected. Only the argv string changes;
    /// the `Path` itself keeps its verbatim form, so Rust-side fs calls and
    /// the path comparisons against what the catalog stores are untouched.
    /// On other platforms the path passes through byte for byte.
    fn git_path_arg(path: &Path) -> String {
        let spelling = path.to_string_lossy();
        #[cfg(windows)]
        if let Some(rest) = spelling.strip_prefix(r"\\?\") {
            return match rest.strip_prefix("UNC\\") {
                Some(unc) => format!(r"\\{unc}"),
                None => rest.to_string(),
            };
        }
        spelling.into_owned()
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
            agent_id: Some(AgentRef::adapter("codex")),
        }];

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, tabs));
        store.flush_now();

        let restored = restore(&db_path, Path::new("/nonexistent/fallback"));
        assert_eq!(restored.tabs[0].agent_id, Some(AgentRef::adapter("codex")));
    }

    /// A worktree the user deleted from disk restores nothing, not the
    /// default strip. Handing back a Chat and a Terminal for a directory
    /// that is gone only builds a pane whose spawn fails, and a failed pane
    /// counts as live work in `requires_close_confirmation` — which is what
    /// left the centre pane bolted to the dead worktree. `write_layout`
    /// already refuses to persist this case, so the read side has to agree
    /// with it.
    #[test]
    fn a_worktree_whose_directory_is_gone_restores_no_tabs() {
        let dir = TempDir::new();
        let db_path = dir.db_path("missing-worktree");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");

        let store = SessionStore::open(&db_path);
        store.schedule(layout(&working_directory, three_tabs()));
        store.flush_now();
        assert_eq!(
            store.restore_tabs_for(&working_directory).tabs.len(),
            3,
            "the fixture must start from a worktree that really has tabs"
        );

        std::fs::remove_dir_all(&working_directory).expect("delete the checkout");

        let restored = store.restore_tabs_for(&working_directory);
        assert!(
            restored.tabs.is_empty(),
            "a worktree whose directory is gone restores nothing, not the \
             default Chat and Terminal strip: {:?}",
            restored.tabs
        );
        assert!(restored.tab_states.is_empty());
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
            chat_draft: String::new(),
            browser_url: String::new(),
            editor_path: String::new(),
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
        let worktree_id = store.persisted_worktree_id(&working_directory);
        let written = db
            .tab_states_of_worktree(&worktree_id)
            .expect("dump written tab state");
        assert_eq!(written.len(), 1);
        assert!(written[0].state.contains("horizontal"));

        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(restored.tab_states, vec![state]);
        assert_eq!(restored.tabs[0].title, "Terminal");
    }

    /// #125 (spec R6.5): the captured address has to survive the write, not
    /// just the struct. `encode` builds a bounded copy field by field, so a
    /// field added to the struct and forgotten there is dropped on the way to
    /// disk with nothing else to notice.
    #[test]
    fn a_browser_tabs_address_survives_the_round_trip_to_disk() {
        let dir = TempDir::new();
        let db_path = dir.db_path("browser-url-roundtrip");
        let working_directory = dir.0.join("checkout");
        std::fs::create_dir_all(&working_directory).expect("checkout dir");
        let state = SessionTabState {
            root_id: Some(0),
            pane_events: Vec::new(),
            scrollback: std::collections::BTreeMap::new(),
            chat_draft: String::new(),
            browser_url: "https://example.org/probe".into(),
            editor_path: String::new(),
        };
        let layout = SessionLayout {
            working_directory: working_directory.clone(),
            branch: "main".into(),
            tabs: vec![SessionTab {
                id: "browser".into(),
                title: "Browser".into(),
                kind: "browser".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![state.clone()],
        };

        let store = SessionStore::open(&db_path);
        store.schedule(layout);
        store.flush_now();

        let db = AppDatabase::open(&db_path).expect("reopen state database");
        let worktree_id = store.persisted_worktree_id(&working_directory);
        let written = db
            .tab_states_of_worktree(&worktree_id)
            .expect("dump written tab state");
        assert!(
            written[0].state.contains("example.org/probe"),
            "the address must reach the stored JSON, not just the in-memory struct: {}",
            written[0].state
        );

        let restored = restore(&db_path, Path::new("/tmp"));
        assert_eq!(
            restored.tab_states[0].browser_url,
            "https://example.org/probe"
        );
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
            // Inside `settings_ranges::TERMINAL_FONT_SIZE` (12..=18, narrowed
            // from 9..=24 on 2026-09-05 with the stepper fix) and not the
            // default 13: `AppDatabase::settings` clamps on load, so a value
            // above the range would come back as 18 and prove nothing about
            // the round trip.
            terminal_font_size: 16,
            base_color: BaseColor::Neutral,
            control_socket_enabled: false,
            updates_enabled: true,
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
            ollama_show_in_bar: true,
            refresh_interval_min: 11,
            opencode_workspace_id_override: "wrk_main".into(),
            translucency: true,
            // Deliberately neither default nor out of range: this round-trip
            // is the only place that proves a dragged width -- and, #323, the
            // dragged centre split -- survives the SessionStore layer, not
            // just `AppDatabase`.
            sidebar_width: 300,
            right_panel_width: 500,
            center_split_ratio: 610,
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
                ("appearance.baseColor".into(), "neutral".into()),
                ("appearance.centerSplitRatio".into(), "610".into()),
                    ("appearance.rightPanelWidth".into(), "500".into()),
                ("appearance.sidebarWidth".into(), "300".into()),
                ("appearance.terminalFontSize".into(), "16".into()),
                ("appearance.theme".into(), "dark".into()),
                ("appearance.translucency".into(), "true".into()),
                ("appearance.uiFontSize".into(), "17".into()),
                ("chat.limitHistory".into(), "false".into()),
                ("chat.retentionCount".into(), "37".into()),
                ("controlSocket.enabled".into(), "false".into()),
                ("general.autoNaming".into(), "true".into()),
                ("general.summarizerAgent".into(), "codex".into()),
                ("session.resumeAgentSessions".into(), "false".into()),
                ("updates.enabled".into(), "true".into()),
                ("usage.claudeVisible".into(), "false".into()),
                ("usage.codexVisible".into(), "false".into()),
                ("usage.ollamaVisible".into(), "true".into()),
                (
                    "usage.opencodeGo.workspaceIdOverride".into(),
                    "wrk_main".into(),
                ),
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
    fn an_old_blob_predating_chat_draft_still_decodes() {
        // F-CORE-WSP-07: the row wants a rejected/versioned scheme for
        // incompatible blobs; the counter-argument is that this format
        // evolves additively via `#[serde(default)]` instead, and the one
        // real historical field-add (`chat_draft`, F-CORE-WSP-08) proves it.
        // This is a hand-written blob in the exact shape `encode()` produced
        // *before* `chat_draft` existed -- not today's own `encode()` output
        // with a field stripped out -- so it genuinely exercises "an old
        // blob still decodes under new code", not a tautology.
        let old_blob =
            r#"{"root_id":3,"pane_events":[{"Close":{"id":7}}],"scrollback":{"2":[104,105]}}"#;
        let decoded = SessionTabState::decode(old_blob).expect("old blob must still decode");
        assert_eq!(decoded.root_id, Some(3));
        assert_eq!(decoded.pane_events, vec![PaneEvent::Close { id: 7 }]);
        assert_eq!(decoded.scrollback.get(&2), Some(&vec![104u8, 105u8]));
        assert_eq!(
            decoded.chat_draft, "",
            "missing chat_draft must default rather than fail decode"
        );
    }

    #[test]
    fn reordered_keys_decode_to_an_identical_state() {
        // F-CORE-WSP-07's "canonical sorted JSON" clause is argued not
        // required because nothing in this codebase diffs or hashes the
        // persisted blob. Prove that directly: two blobs with the same
        // fields in different key order must decode to the exact same
        // struct (derived `PartialEq`), not merely both succeed.
        let forward = r#"{"root_id":1,"pane_events":[],"scrollback":{},"chat_draft":"hi"}"#;
        let reordered = r#"{"chat_draft":"hi","scrollback":{},"pane_events":[],"root_id":1}"#;
        let a = SessionTabState::decode(forward).expect("forward-order blob decodes");
        let b = SessionTabState::decode(reordered).expect("reordered blob decodes");
        assert_eq!(a, b, "key order must not affect the decoded value");
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

    /// Polls until `predicate` holds, or the deadline passes.
    ///
    /// The debounce tests used to `sleep` a fixed 250 ms and then assert. A
    /// sleep only guarantees that *wall-clock* time passed, not that the
    /// writer thread was ever scheduled — and `cargo test --workspace` runs
    /// roughly fifty test binaries at once, which is exactly when it is not.
    /// Observed under that load: the whole 250 ms elapsed with the worker
    /// never having run, so `writes()` was still `0`.
    ///
    /// The deadline is deliberately generous (30 s against a 60 ms debounce).
    /// Measured on this box under `--workspace` load, the two flushes together
    /// took ~9 s of wall clock — so a tight bound would just reintroduce the
    /// flake in a new place. A healthy run still returns in milliseconds,
    /// because this polls rather than sleeps; the deadline only bounds how
    /// long a pathological run waits before failing with a clear message.
    fn wait_until(deadline: Duration, mut predicate: impl FnMut() -> bool) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        predicate()
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

        // Wait for the burst to actually land, instead of sleeping and hoping.
        // This assertion also replaces an earlier `writes() <= 2`, which was
        // satisfied by `0` — so the one failure that matters here, the worker
        // never running at all, *passed* that check and only surfaced further
        // down as a confusing `left: 0, right: 2`.
        assert!(
            wait_until(Duration::from_secs(30), || store.writes() >= 1),
            "the debounce worker never flushed the burst at all"
        );
        assert_eq!(
            store.writes(),
            1,
            "a burst of 10 changes inside one window must collapse to a single write"
        );

        // A later change flushes again once the window has passed.
        store.schedule(layout(&dir.0, three_tabs()));
        assert!(
            wait_until(Duration::from_secs(30), || store.writes() >= 2),
            "a quiet gap must let the next change flush on its own; wrote {}",
            store.writes()
        );
        assert_eq!(
            store.writes(),
            2,
            "the later change must add exactly one more write, not a burst"
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
    fn missing_worktree_without_a_row_does_not_fall_back_to_another_row() {
        let dir = TempDir::new();
        let database = dir.db_path("missing-worktree-identity");
        let project_root = dir.0.join("repo");
        let missing_path = dir.0.join("gone-worktree");
        std::fs::create_dir_all(&project_root).expect("project root");
        assert!(!missing_path.is_dir(), "the selected worktree is absent");

        let db = AppDatabase::open(&database).expect("open database");
        db.save_project(&ProjectRecord::new(
            "project",
            "Project",
            project_root.to_string_lossy(),
        ))
        .expect("save project");
        db.save_worktree(&WorktreeRecord::new(
            "other-worktree",
            "project",
            "other",
            dir.0.join("other-worktree").to_string_lossy(),
        ))
        .expect("save other worktree");

        assert_eq!(
            worktree_id_for_database(&db, &missing_path).expect("resolve worktree"),
            None,
            "an absent worktree must not fall back to another row's identity"
        );
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
        // F-SID-11: a plain folder has no git worktrees to discover, but the
        // sidebar still needs a row to show branch/path/Primary/status/
        // comment for it, matching the git project's baseline -- so a
        // single synthetic primary "worktree" (the folder root itself) is
        // synthesized rather than leaving the project with none at all.
        assert_eq!(
            catalog.projects()[1].worktrees,
            vec![CatalogWorktree {
                branch: String::new(),
                path: canonical_path(&plain_root),
                is_primary: true,
            }]
        );
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
        let status = std::process::Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(git_path_arg(&bare_root))
            .status()
            .expect("git init --bare");
        // Asserted, not ignored: a silently failed `init` used to surface
        // three lines further down as `catalog.add` reporting os error 2,
        // which reads like a defect in the code under test.
        assert!(status.success(), "git init --bare failed: {status}");

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
            &["config", "user.email", "sirio-tests@example.com"],
        );
        run_git(&primary, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(primary.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&primary, &["add", "README"]);
        run_git(&primary, &["commit", "--quiet", "-m", "fixture"]);
        let status = std::process::Command::new("git")
            .args(["worktree", "add", "--quiet", "-b", "linked"])
            .arg(git_path_arg(&linked))
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
                worktree_id(&projects[0].id, Path::new(&worktrees[0].path)),
                worktree_id(&projects[0].id, Path::new(&worktrees[1].path)),
            ]
        );
    }

    #[test]
    fn a_broken_linked_worktree_is_not_promoted_to_a_project() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let linked = dir.0.join("repo-linked");
        std::fs::create_dir_all(&primary).expect("repo dir");
        run_git(&primary, &["init", "--quiet", "-b", "main"]);
        run_git(
            &primary,
            &["config", "user.email", "sirio-tests@example.com"],
        );
        run_git(&primary, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(primary.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&primary, &["add", "README"]);
        run_git(&primary, &["commit", "--quiet", "-m", "fixture"]);

        let status = std::process::Command::new("git")
            .args(["worktree", "add", "--quiet", "-b", "linked"])
            .arg(git_path_arg(&linked))
            .current_dir(&primary)
            .status()
            .expect("git worktree add");
        assert!(status.success(), "git worktree add failed: {status}");

        std::fs::write(
            linked.join(".git"),
            format!(
                "gitdir: {}\n",
                dir.0.join("missing-worktree-admin").display()
            ),
        )
        .expect("break linked worktree gitdir");

        let database = dir.db_path("broken-linked-worktree");
        let store = SessionStore::open(&database);
        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&primary).expect("discover primary repository"));
        store.schedule_catalog(&catalog);

        // Seed the duplicate rows that the old startup save produced, then
        // make sure a relaunch drops the stale top-level project again.
        let db = AppDatabase::open(&database).expect("open database");
        db.save_project(&ProjectRecord::new(
            "phantom-project",
            "repo-linked",
            linked.to_string_lossy(),
        ))
        .expect("save stale project");
        db.save_worktree(&WorktreeRecord::new(
            "phantom-worktree",
            "phantom-project",
            "main",
            linked.to_string_lossy(),
        ))
        .expect("save stale project worktree");
        drop(db);

        let restored = restore_catalog(&database);
        assert_eq!(
            restored.projects.len(),
            1,
            "a stale linked worktree project must be merged away on restore"
        );
        assert!(
            restored
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("worktree")),
            "dropping the stale project must be logged: {:?}",
            restored.diagnostics
        );
        store.schedule_catalog(&ProjectCatalog::from_restored(
            restored.projects,
            restored.settings,
        ));

        // This is the startup save that used to derive a second project from
        // the broken linked worktree after discovery failed.
        store.schedule(layout(&linked, Vec::new()));
        store.flush_now();

        let db = AppDatabase::open(&database).expect("reopen database");
        let projects = db.projects().expect("read projects");
        assert_eq!(
            projects.len(),
            1,
            "broken worktree must not become a project"
        );
        assert_eq!(projects[0].root_path, primary.to_string_lossy());
        let worktrees = db
            .worktrees_of_project(&projects[0].id)
            .expect("read project worktrees");
        assert!(
            worktrees
                .iter()
                .any(|worktree| worktree.path == linked.to_string_lossy()),
            "the broken checkout remains owned by its repository project"
        );
    }

    #[test]
    fn worktree_ids_and_tabs_survive_a_preceding_worktree_removal() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let preceding = dir.0.join("repo-aaa");
        let linked = dir.0.join("repo-zzz");
        std::fs::create_dir_all(&primary).expect("repo dir");
        run_git(&primary, &["init", "--quiet", "-b", "main"]);
        run_git(
            &primary,
            &["config", "user.email", "sirio-tests@example.com"],
        );
        run_git(&primary, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(primary.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&primary, &["add", "README"]);
        run_git(&primary, &["commit", "--quiet", "-m", "fixture"]);

        for (branch, path) in [("preceding", &preceding), ("linked", &linked)] {
            let status = std::process::Command::new("git")
                .args(["worktree", "add", "--quiet", "-b", branch])
                .arg(git_path_arg(path))
                .current_dir(&primary)
                .status()
                .expect("git worktree add");
            assert!(status.success(), "git worktree add failed: {status}");
        }

        let database = dir.db_path("worktree-index-shift");
        let store = SessionStore::open(&database);
        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&primary).expect("discover primary repository"));
        store.schedule_catalog(&catalog);
        store.schedule(layout(
            &linked,
            vec![SessionTab {
                id: "linked-terminal".into(),
                title: "Linked terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
        ));
        store.flush_now();

        let before = AppDatabase::open(&database).expect("open database before refresh");
        let original_id = before
            .worktrees_of_project(&catalog.projects()[0].id)
            .expect("read initial worktrees")
            .into_iter()
            .find(|worktree| worktree.path == linked.to_string_lossy())
            .expect("linked worktree row")
            .id;
        assert_eq!(
            original_id,
            worktree_id(&catalog.projects()[0].id, &linked),
            "the linked worktree id is derived from its path, not its catalog index"
        );
        assert_eq!(
            before
                .tabs_of_worktree(&original_id)
                .expect("read initial tabs")
                .len(),
            1
        );

        let status = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(git_path_arg(&preceding))
            .current_dir(&primary)
            .status()
            .expect("git worktree remove");
        assert!(status.success(), "git worktree remove failed: {status}");

        let mut refreshed = ProjectCatalog::default();
        assert!(refreshed.add(&primary).expect("rediscover primary repository"));
        store.schedule_catalog(&refreshed);

        let after = AppDatabase::open(&database).expect("open database after refresh");
        let refreshed_row = after
            .worktrees_of_project(&refreshed.projects()[0].id)
            .expect("read refreshed worktrees")
            .into_iter()
            .find(|worktree| worktree.path == linked.to_string_lossy())
            .expect("refreshed linked worktree row");
        assert_eq!(
            refreshed_row.id, original_id,
            "refresh must preserve a worktree id when an earlier row disappears"
        );

        let restored = store.restore_tabs_for(&linked);
        assert_eq!(
            restored.tabs,
            vec![SessionTab {
                id: "linked-terminal".into(),
                title: "Linked terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
            "the linked worktree must restore its own tabs after its Git index shifts"
        );
        assert_eq!(
            after
                .tabs_of_worktree(&original_id)
                .expect("read preserved tabs")
                .len(),
            1,
            "refresh must not cascade-delete tabs under the preserved worktree id"
        );
    }

    #[test]
    fn inserting_a_preceding_worktree_preserves_its_id_and_layout() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let preceding = dir.0.join("repo-aaa");
        let linked = dir.0.join("repo-zzz");
        for path in [&primary, &preceding, &linked] {
            std::fs::create_dir_all(path).expect("create worktree fixture");
        }

        let project = CatalogProject {
            id: "project".into(),
            name: "Project".into(),
            root_path: primary.clone(),
            is_git: true,
            worktrees: vec![
                CatalogWorktree {
                    branch: "main".into(),
                    path: primary.clone(),
                    is_primary: true,
                },
                CatalogWorktree {
                    branch: "linked".into(),
                    path: linked.clone(),
                    is_primary: false,
                },
            ],
        };
        let mut shifted_project = project.clone();
        shifted_project.worktrees.insert(
            1,
            CatalogWorktree {
                branch: "preceding".into(),
                path: preceding,
                is_primary: false,
            },
        );

        let baseline_database = dir.db_path("baseline");
        let baseline_store = SessionStore::open(&baseline_database);
        baseline_store.schedule_catalog(&ProjectCatalog::from_projects(vec![project]));
        let baseline_id = AppDatabase::open(&baseline_database)
            .expect("open baseline database")
            .worktree_by_path(&linked.to_string_lossy())
            .expect("read baseline worktree")
            .expect("baseline linked worktree")
            .id;

        let shifted_database = dir.db_path("shifted");
        let shifted_store = SessionStore::open(&shifted_database);
        shifted_store.schedule_catalog(&ProjectCatalog::from_projects(vec![shifted_project]));
        shifted_store.schedule(layout(
            &linked,
            vec![SessionTab {
                id: "linked-tab".into(),
                title: "Linked terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
        ));
        shifted_store.flush_now();

        let shifted_db = AppDatabase::open(&shifted_database).expect("open shifted database");
        let shifted_id = shifted_db
            .worktree_by_path(&linked.to_string_lossy())
            .expect("read shifted worktree")
            .expect("shifted linked worktree")
            .id;
        assert_eq!(
            shifted_id, baseline_id,
            "a worktree id must not change when a preceding catalog row is inserted"
        );
        assert_eq!(
            restore(&shifted_database, Path::new("/tmp")).tabs[0].title,
            "Linked terminal",
            "the shifted catalog must still restore the layout owned by linked"
        );
    }

    #[test]
    fn refreshing_a_project_tracks_external_worktrees_and_preserves_path_ids() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let removed = dir.0.join("repo-removed");
        let mounted = dir.0.join("repo-mounted");
        let survivor = dir.0.join("repo-survivor");
        let added = dir.0.join("repo-added");
        std::fs::create_dir_all(&primary).expect("repo dir");
        run_git(&primary, &["init", "--quiet", "-b", "main"]);
        run_git(
            &primary,
            &["config", "user.email", "sirio-tests@example.com"],
        );
        run_git(&primary, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(primary.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&primary, &["add", "README"]);
        run_git(&primary, &["commit", "--quiet", "-m", "fixture"]);

        for (branch, path) in [
            ("removed", &removed),
            ("mounted", &mounted),
            ("survivor", &survivor),
        ] {
            let status = std::process::Command::new("git")
                .args(["worktree", "add", "--quiet", "-b", branch])
                .arg(git_path_arg(path))
                .current_dir(&primary)
                .status()
                .expect("git worktree add");
            assert!(status.success(), "git worktree add failed: {status}");
        }

        let database = dir.db_path("runtime-refresh");
        let store = SessionStore::open(&database);
        let mut catalog = ProjectCatalog::default();
        assert!(catalog.add(&primary).expect("discover primary repository"));
        let project_id = catalog.projects()[0].id.clone();
        store.schedule_catalog(&catalog);

        let before = AppDatabase::open(&database).expect("open database before refresh");
        let survivor_id = before
            .worktrees_of_project(&project_id)
            .expect("read initial worktrees")
            .into_iter()
            .find(|worktree| worktree.path == survivor.to_string_lossy())
            .expect("survivor worktree row")
            .id;

        let status = std::process::Command::new("git")
            .args(["worktree", "add", "--quiet", "-b", "added"])
            .arg(git_path_arg(&added))
            .current_dir(&primary)
            .status()
            .expect("git worktree add");
        assert!(status.success(), "git worktree add failed: {status}");
        catalog
            .refresh_project(&project_id)
            .expect("refresh after external add");
        assert!(catalog.projects()[0]
            .worktrees
            .iter()
            .any(|worktree| worktree.path == added));
        store.schedule_catalog(&catalog);

        let status = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(git_path_arg(&removed))
            .current_dir(&primary)
            .status()
            .expect("git worktree remove");
        assert!(status.success(), "git worktree remove failed: {status}");
        catalog
            .refresh_project(&project_id)
            .expect("refresh after external removal");

        let worktrees = &catalog.projects()[0].worktrees;
        assert!(worktrees.iter().any(|worktree| worktree.path == added));
        assert!(!worktrees.iter().any(|worktree| worktree.path == removed));
        let survivor_row = worktrees
            .iter()
            .find(|worktree| worktree.path == survivor)
            .expect("survivor remains in the refreshed catalog");
        assert!(
            !catalog.is_worktree_missing(&survivor_row.path),
            "the surviving checkout is not missing"
        );

        // A second refresh receives the path of the terminal that was mounted
        // before Git removed its worktree. The row is retained and marked so
        // the running terminal can finish without making the stale checkout
        // selectable as if it still existed.
        let status = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(git_path_arg(&mounted))
            .current_dir(&primary)
            .status()
            .expect("git worktree remove");
        assert!(status.success(), "git worktree remove failed: {status}");
        catalog
            .refresh_project_with_mounted_worktrees(&project_id, std::slice::from_ref(&mounted))
            .expect("refresh while the removed terminal remains mounted");
        assert!(catalog.is_worktree_missing(&mounted));
        assert!(!catalog.is_worktree_missing(&survivor));

        store.schedule_catalog(&catalog);
        let after = AppDatabase::open(&database).expect("open database after refresh");
        let persisted_survivor_id = after
            .worktrees_of_project(&project_id)
            .expect("read refreshed worktrees")
            .into_iter()
            .find(|worktree| worktree.path == survivor.to_string_lossy())
            .expect("persisted survivor worktree row")
            .id;
        assert_eq!(persisted_survivor_id, survivor_id);
    }

    /// F-PRJ-17/F-PRJ-18: `default_worktree_base` and
    /// `worktree_location_override` round-trip through `write_catalog` and
    /// `restore_catalog` the same way `color_hex`/`display_name` already do
    /// above. Before this, `CatalogProjectSettings` had no fields for
    /// either — `write_catalog` never set the already-migrated
    /// `project.default_worktree_base`/`worktree_location_override`
    /// columns, and `restore_catalog` never read them back, so a value
    /// set through `update_project_settings` was silently dropped at the
    /// next save and could never survive a relaunch.
    #[test]
    fn default_worktree_base_and_location_override_round_trip_through_the_catalog_store() {
        let dir = TempDir::new();
        let root = dir.0.join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        run_git(&root, &["init", "--quiet"]);
        run_git(&root, &["config", "user.email", "sirio-tests@example.com"]);
        run_git(&root, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(root.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&root, &["add", "README"]);
        run_git(&root, &["commit", "--quiet", "-m", "fixture"]);

        let database = dir.db_path("worktree-defaults");
        let store = SessionStore::open(&database);
        let discovered = discover_project(&root).expect("discover the fixture repo");
        let project = catalog_project(&root, discovered);
        let mut catalog = ProjectCatalog::from_projects(vec![project.clone()]);
        catalog
            .update_project_settings(
                &project.id,
                CatalogProjectSettings {
                    default_worktree_base: Some("develop".to_string()),
                    worktree_location_override: Some("/srv/worktrees".to_string()),
                    ..CatalogProjectSettings::default()
                },
            )
            .expect("project is in the catalog");
        store.schedule_catalog(&catalog);

        let restored = restore_catalog(&database);
        let restored_settings = restored
            .settings
            .get(&project.id)
            .expect("project identity settings restore with the canonical id");
        assert_eq!(
            restored_settings.default_worktree_base.as_deref(),
            Some("develop"),
            "the default worktree base survives a write/restore round trip"
        );
        assert_eq!(
            restored_settings.worktree_location_override.as_deref(),
            Some("/srv/worktrees"),
            "the worktree location override survives a write/restore round trip"
        );
    }

    /// F-CTRL-WORK-01: a comment persisted on a worktree row (as
    /// `persist_worktree_comment` does from `worktree.set`) must survive a
    /// later `schedule_catalog` call built from an in-memory
    /// `ProjectCatalog` that has no notion of comments at all — exactly
    /// what `main()` does at every startup via its catalog-normalization
    /// call. Before this fix, `write_catalog` rebuilt every worktree row
    /// from scratch with `WorktreeRecord::new()` (comment: None) and
    /// upserted it, wiping the column back to NULL on every boot.
    #[test]
    fn schedule_catalog_preserves_a_previously_persisted_worktree_comment() {
        let dir = TempDir::new();
        let root = dir.0.join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        run_git(&root, &["init", "--quiet"]);
        run_git(&root, &["config", "user.email", "sirio-tests@example.com"]);
        run_git(&root, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(root.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&root, &["add", "README"]);
        run_git(&root, &["commit", "--quiet", "-m", "fixture"]);

        let database = dir.db_path("worktree-comment");
        let store = SessionStore::open(&database);
        let discovered = discover_project(&root).expect("discover the fixture repo");
        let project = catalog_project(&root, discovered);
        let catalog = ProjectCatalog::from_projects(vec![project.clone()]);
        // First boot: normalizes the catalog, creating the worktree row.
        store.schedule_catalog(&catalog);

        // Simulate `persist_worktree_comment`: directly upsert a comment
        // onto the now-existing worktree row, as the control handler does.
        let db = AppDatabase::open(&database).expect("reopen database");
        let worktree_id = db
            .worktrees_of_project(&project.id)
            .expect("read worktrees")
            .into_iter()
            .next()
            .expect("primary worktree row exists")
            .id;
        let mut record = db
            .worktree_by_path(&root.to_string_lossy())
            .expect("query by path")
            .expect("worktree row exists");
        record.comment = Some("needs review".to_string());
        db.save_worktree(&record).expect("persist the comment");
        drop(db);

        // A later boot (or any other schedule_catalog call) must not wipe
        // the comment back to NULL.
        store.schedule_catalog(&catalog);

        let reopened = AppDatabase::open(&database).expect("reopen database again");
        let worktrees = reopened
            .worktrees_of_project(&project.id)
            .expect("read worktrees after re-schedule");
        let worktree = worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .expect("same worktree row still present");
        assert_eq!(
            worktree.comment.as_deref(),
            Some("needs review"),
            "schedule_catalog must not clobber a previously persisted comment"
        );
    }

    /// #323: the pane flag rides the same hazard the comment above does --
    /// `schedule_catalog` re-derives the row from a catalog that has never
    /// heard of it.
    #[test]
    fn the_secondary_pane_flag_round_trips_and_survives_a_catalog_rebuild() {
        let dir = TempDir::new();
        let root = dir.0.join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        run_git(&root, &["init", "--quiet"]);
        run_git(&root, &["config", "user.email", "sirio-tests@example.com"]);
        run_git(&root, &["config", "user.name", "Sirio Tests"]);
        std::fs::write(root.join("README"), "catalog fixture\n").expect("fixture file");
        run_git(&root, &["add", "README"]);
        run_git(&root, &["commit", "--quiet", "-m", "fixture"]);

        let database = dir.db_path("secondary-pane-flag");
        let store = SessionStore::open(&database);
        let discovered = discover_project(&root).expect("discover the fixture repo");
        let catalog = ProjectCatalog::from_projects(vec![catalog_project(&root, discovered)]);
        store.schedule_catalog(&catalog);

        assert!(
            !store.secondary_pane_open_for(&root),
            "a worktree that never opened the pane reads closed"
        );

        store.save_secondary_pane_open(&root, true);
        assert!(store.secondary_pane_open_for(&root), "the flag round-trips");

        // Any later boot re-runs this; it must not close the pane.
        store.schedule_catalog(&catalog);
        assert!(
            store.secondary_pane_open_for(&root),
            "schedule_catalog must not clobber the pane flag"
        );

        store.save_secondary_pane_open(&root, false);
        assert!(
            !store.secondary_pane_open_for(&root),
            "closing the pane persists too"
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

    /// F-CHG-02/03/09: the shared root cause of three findings from a
    /// finish-line critic pass -- a transient permission fault on `.git`
    /// (unreadable, then restored) used to make `restore_catalog` drop the
    /// project entirely rather than keep its last-known state, which in
    /// turn destroyed the sidebar row and desynced it from the
    /// independently-restored working directory before the Changes/Files
    /// surfaces' own "Git unavailable" + Retry panels ever got a chance to
    /// render. This is Linux/Unix-only (permission bits), matching the
    /// "platform gate" the OS-touching fix requires.
    #[test]
    #[cfg(unix)]
    fn restore_keeps_a_project_whose_git_is_transiently_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new();
        let root = checkout(&dir.0, "flaky");
        std::fs::write(root.join("a.txt"), "hi").expect("write file");
        run_git(&root, &["add", "a.txt"]);
        run_git(
            &root,
            &[
                "-c",
                "user.email=a@b.c",
                "-c",
                "user.name=a",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        let db_path = dir.db_path("flaky-git");
        let store = SessionStore::open(&db_path);
        let mut catalog = ProjectCatalog::default();
        catalog.add(&root).expect("discover the healthy repo");
        assert!(
            catalog.projects()[0].is_git,
            "precondition: a real git checkout discovers as git"
        );
        store.schedule_catalog(&catalog);
        store.flush_now();
        drop(store);

        let git_dir = root.join(".git");
        let original_mode = std::fs::metadata(&git_dir)
            .expect("stat .git")
            .permissions()
            .mode();
        std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(0o000))
            .expect("chmod .git unreadable");

        let restored = restore_catalog(&db_path);

        // Restore permissions immediately, win or lose, so the temp dir
        // can still be cleaned up and no assertion below can leave the
        // fixture broken for anything that runs after it.
        std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(original_mode))
            .expect("restore .git permissions");

        assert_eq!(
            restored.projects.len(),
            1,
            "a transient git fault must not drop a durable project from the catalog"
        );
        let project = &restored.projects[0];
        assert_eq!(project.root_path, root.canonicalize().unwrap());
        assert!(
            project.is_git,
            "the project's last-known git status must be preserved, not reset"
        );
        assert_eq!(
            project.worktrees.len(),
            1,
            "the last persisted worktree row must survive, so the sidebar row and the \
             independently-restored working directory stay in sync"
        );
        assert_eq!(project.worktrees[0].path, root.canonicalize().unwrap());
        assert!(
            restored
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("could not be refreshed")),
            "the fault is still logged, just not treated as the project vanishing: {:?}",
            restored.diagnostics
        );
        assert!(
            !restored
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("vanished")),
            "a permission fault is not the same as the directory disappearing: {:?}",
            restored.diagnostics
        );

        // And once the fault clears, a later restore is the normal,
        // fully-rediscovered path again -- this project was never actually
        // lost from the database.
        let recovered = restore_catalog(&db_path);
        assert_eq!(recovered.projects.len(), 1);
        assert!(recovered.projects[0].is_git);
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

    /// Places a (fake) binary inside a checkout, as `target/debug/sirio`
    /// would be after a build.
    fn built_binary(checkout_root: &Path) -> PathBuf {
        let exe = checkout_root.join("target/debug/sirio");
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
        let checkout_a = checkout(&dir.0, "sirio-main");
        let checkout_b = checkout(&dir.0, "sirio-p57-dbscope");
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

    /// #125 (spec R6.1): the profile is a sibling of the database, not a
    /// child of it -- WebView2 owns everything under its user-data folder.
    #[test]
    fn the_browser_profile_sits_beside_its_database() {
        let database = Path::new("/state/checkouts/sirio-abcd1234/sirio.sqlite");
        assert_eq!(
            browser_profile_path_for(database),
            Path::new("/state/checkouts/sirio-abcd1234/browser")
        );
    }

    /// The wrapper, not just the rule it delegates to: `browser_profile_path`
    /// must resolve against the *live* database path, and as its sibling.
    /// Breaking that -- nesting the profile inside the database's own path --
    /// is invisible to the tests that call `browser_profile_path_for`
    /// directly, which is exactly how it would reach a release.
    #[test]
    fn the_live_profile_path_is_the_live_databases_sibling() {
        let database = database_path();
        let profile = browser_profile_path();
        assert_eq!(
            profile.parent(),
            database.parent(),
            "the profile must sit beside the database, not inside it"
        );
        assert_eq!(profile.file_name(), Some(std::ffi::OsStr::new("browser")));
    }

    /// #125 (spec R6.1): the profile inherits the database's scoping, so two
    /// checkouts of the app cannot end up sharing one WebView2 profile
    /// directory while their session state is correctly separate.
    #[test]
    fn two_checkouts_do_not_share_a_browser_profile() {
        let dir = TempDir::new();
        let checkout_a = checkout(&dir.0, "sirio-main");
        let checkout_b = checkout(&dir.0, "sirio-r61-profile");
        let exe_a = built_binary(&checkout_a);
        let exe_b = built_binary(&checkout_b);

        let profile_a = browser_profile_path_for(&database_path_for(&dir.0, &exe_a, &dir.0, true));
        let profile_b = browser_profile_path_for(&database_path_for(&dir.0, &exe_b, &dir.0, true));

        assert_ne!(
            profile_a, profile_b,
            "each checkout must own its own browser profile"
        );
        assert!(
            profile_a.starts_with(dir.0.join("checkouts")),
            "a development checkout's profile is scoped, not user-wide: {}",
            profile_a.display()
        );
    }

    /// The installed case: no checkout, so the profile is the one stable
    /// user-wide location, beside the one stable database.
    #[test]
    fn an_installed_binary_keeps_one_stable_browser_profile() {
        let dir = TempDir::new();
        let installed = dir.0.join("Applications").join("sirio");
        std::fs::create_dir_all(installed.parent().expect("parent")).expect("install dir");
        std::fs::write(&installed, b"binary").expect("install binary");

        let database = database_path_for(&dir.0, &installed, &dir.0, false);
        assert_eq!(browser_profile_path_for(&database), dir.0.join("browser"));
    }

    #[test]
    fn the_scoped_path_is_human_readable_and_pinned_in_shape() {
        let dir = TempDir::new();
        let checkout_root = checkout(&dir.0, "sirio-rust-p57-dbscope");
        let exe = built_binary(&checkout_root);

        let path = database_path_for(&dir.0, &exe, &dir.0, true);

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("sirio.sqlite"),
            "the file name stays sirio.sqlite"
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
        let Some(suffix) = name.strip_prefix("sirio-rust-p57-dbscope-") else {
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
        let installed = dir.0.join("Applications/Sirio.app/Contents/MacOS/sirio");
        std::fs::create_dir_all(installed.parent().expect("bundle dir")).expect("bundle");
        std::fs::write(&installed, b"").expect("binary");

        let path = database_path_for(&dir.0, &installed, &dir.0, true);
        assert_eq!(
            path,
            dir.0.join("sirio.sqlite"),
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
        let checkout_root = checkout(&dir.0, "sirio-main");
        let shared_exe = dir.0.join("shared-target/debug/sirio");
        std::fs::create_dir_all(shared_exe.parent().expect("target dir")).expect("target");
        std::fs::write(&shared_exe, b"").expect("exe file");

        let scoped = database_path_for(&dir.0, &shared_exe, &checkout_root, true);
        assert_ne!(scoped, dir.0.join("sirio.sqlite"));
        assert!(scoped.starts_with(dir.0.join("checkouts")));

        // Release builds (cwd_fallback off) must never be diverted by the
        // CWD: a shipped binary launched from inside a repo keeps the
        // stable path.
        let stable = database_path_for(&dir.0, &shared_exe, &checkout_root, false);
        assert_eq!(stable, dir.0.join("sirio.sqlite"));
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
            dir.0.join("sirio.sqlite"),
            "a git worktree gets its own database"
        );
    }

    #[test]
    fn a_path_absent_from_git_discovery_does_not_alias_the_primary_worktree() {
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let missing = dir.0.join("missing-worktree");
        let discovered = DiscoveredProject {
            is_git: true,
            worktrees: vec![sirio_project::DiscoveredWorktree {
                path: primary.clone(),
                head: None,
                branch: Some("main".into()),
                is_primary: true,
                locked: false,
                prunable: false,
            }],
        };

        let (root, project, worktree) = catalog_ids_for_discovered_path(&missing, &discovered);
        assert_eq!(root, missing);
        assert_eq!(project, project_id(&missing));
        assert_ne!(
            worktree,
            worktree_id(&project_id(&primary), &primary),
            "a missing discovery entry must not become the primary worktree"
        );
    }

    #[test]
    fn a_prunable_discovered_worktree_is_not_shown() {
        // `git worktree list --porcelain` keeps reporting a worktree whose
        // checkout directory was deleted without `git worktree remove` --
        // marked `prunable` (see discovery.rs's parser tests for the exact
        // reason text git emits). `catalog_project` must not turn that entry
        // into a sidebar row: it points at a directory that no longer
        // exists locally.
        let dir = TempDir::new();
        let primary = dir.0.join("repo");
        let deleted = dir.0.join("worktree-agent-a189089dda476c29a");
        let discovered = DiscoveredProject {
            is_git: true,
            worktrees: vec![
                sirio_project::DiscoveredWorktree {
                    path: primary.clone(),
                    head: None,
                    branch: Some("main".into()),
                    is_primary: true,
                    locked: false,
                    prunable: false,
                },
                sirio_project::DiscoveredWorktree {
                    path: deleted,
                    head: None,
                    branch: Some("fix/popup_error".into()),
                    is_primary: false,
                    locked: false,
                    prunable: true,
                },
            ],
        };

        let project = catalog_project(&primary, discovered);

        assert_eq!(
            project.worktrees.len(),
            1,
            "a prunable worktree is not available locally and must not appear"
        );
        assert_eq!(project.worktrees[0].path, primary);
    }

    #[test]
    fn a_pre_rebrand_state_directory_is_carried_over_not_abandoned() {
        let base = std::env::temp_dir().join(format!("sirio-migrate-{}", std::process::id()));
        let legacy = base.join("TillerRust");
        let current = base.join("Sirio");
        std::fs::create_dir_all(&legacy).expect("seed the pre-rebrand root");
        std::fs::write(legacy.join("sirio.sqlite"), b"session state")
            .expect("seed a database inside it");

        migrate_legacy_state_root(&current);

        assert!(!legacy.exists(), "the old root is moved, not copied");
        assert_eq!(
            std::fs::read(current.join("sirio.sqlite")).expect("the database came across"),
            b"session state",
            "existing session state must survive the rename"
        );

        // Second launch: the new root exists, so nothing is touched.
        std::fs::create_dir_all(&legacy).expect("a stray old root reappears");
        migrate_legacy_state_root(&current);
        assert!(legacy.exists(), "an existing new root makes this a no-op");

        let _ = std::fs::remove_dir_all(&base);
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
            PathBuf::from("/run/user/1000/state/Sirio")
        );

        let fallback =
            std::collections::BTreeMap::from([("HOME".to_string(), "/home/alice".to_string())]);
        assert_eq!(
            app_support_root_for(&fallback),
            PathBuf::from("/home/alice/.local/state/Sirio")
        );
    }
}

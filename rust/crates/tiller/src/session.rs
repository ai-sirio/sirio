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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tiller_persistence::{
    AppDatabase, PersistenceError, ProjectRecord, SidebarState, TabRecord, WorktreeRecord,
};
use tiller_project::{DiscoveredProject, discover_project};

/// How long a burst of changes is held before one write. 500 ms is under the
/// reaction time between discrete user actions (a click then flushes at the
/// next quiet boundary) yet long enough that a burst of programmatic changes
/// collapses into a single write; `flush_now` on quit makes the window
/// lossless.
pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

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
/// The stable path deliberately does NOT point at the shipping Swift app's
/// "Application Support/Tiller" database: pointing the rewrite at it opened
/// a 21 MB production file, ran its own migrations inside it, and left two
/// foreign tables behind. The rewrite keeps its own state until it replaces
/// the Swift app outright.
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
/// one directory per development checkout.
fn app_support_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/TillerRust"))
        .unwrap_or_else(|| std::env::temp_dir().join("TillerRust"))
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
    if use_cwd_fallback
        && let Some(checkout) = find_checkout_root(cwd)
    {
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
        .join(format!("{}-{hash:08x}", sanitize_filename_component(&basename)))
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
    /// Tab title.
    pub title: String,
    /// Surface kind: "chat" or "terminal" (the shell's `TabKind`).
    pub kind: String,
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
                    title: "Chat".into(),
                    kind: "chat".into(),
                    active: false,
                },
                SessionTab {
                    title: "Terminal".into(),
                    kind: "terminal".into(),
                    active: true,
                },
            ],
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectCatalog {
    projects: Vec<CatalogProject>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredCatalog {
    pub projects: Vec<CatalogProject>,
    pub diagnostics: Vec<String>,
}

impl ProjectCatalog {
    pub fn from_projects(projects: Vec<CatalogProject>) -> Self {
        Self { projects }
    }

    pub fn projects(&self) -> &[CatalogProject] {
        &self.projects
    }

    pub fn add(&mut self, path: &Path) -> Result<bool, String> {
        let root_path = path
            .canonicalize()
            .map_err(|error| format!("cannot add {}: {error}", path.display()))?;
        if !root_path.is_dir() {
            return Err(format!("{} is not a directory", root_path.display()));
        }
        if self
            .projects
            .iter()
            .any(|project| {
                root_path == project.root_path
                    || root_path.starts_with(&project.root_path)
                    || project.root_path.starts_with(&root_path)
            })
        {
            return Ok(false);
        }

        let discovered = discover_project(&root_path).map_err(|error| error.to_string())?;
        self.projects.push(catalog_project(&root_path, discovered));
        Ok(true)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let Some(index) = self.projects.iter().position(|project| project.id == id) else {
            return false;
        };
        self.projects.remove(index);
        true
    }
}

fn catalog_project(root_path: &Path, discovered: DiscoveredProject) -> CatalogProject {
    let name = root_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| root_path.to_string_lossy().into_owned());
    let (id, _) = path_ids(root_path);
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
            path: worktree.path,
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
fn path_ids(working_directory: &Path) -> (String, String) {
    let hash = fnv1a64(working_directory.to_string_lossy().as_bytes());
    (
        format!("p-{hash:016x}"),
        format!("w-{hash:016x}"),
    )
}

/// Writes one layout to the database: the project and worktree records
/// (upserted), the worktree's tabs (replaced atomically, with the active
/// flag normalized), and the sidebar selection (read-modify-write so other
/// projects' expansion state survives).
fn write_layout(db: &AppDatabase, layout: &SessionLayout) -> Result<(), PersistenceError> {
    let (project_id, worktree_id) = path_ids(&layout.working_directory);
    let name = layout
        .working_directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| layout.working_directory.to_string_lossy().into_owned());
    let path = layout.working_directory.to_string_lossy().into_owned();
    let branch = if layout.branch.is_empty() {
        "main".to_string()
    } else {
        layout.branch.clone()
    };

    db.save_project(&ProjectRecord::new(&project_id, &name, &path))?;
    db.save_worktree(&WorktreeRecord::new(&worktree_id, &project_id, &branch, &path))?;

    let tabs: Vec<TabRecord> = layout
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| TabRecord {
            id: format!("{worktree_id}-tab-{index}"),
            worktree_id: worktree_id.clone(),
            title: tab.title.clone(),
            kind: tab.kind.clone(),
            order_idx: index as i64,
            is_active: tab.active,
        })
        .collect();
    db.save_tabs(&worktree_id, &tabs)?;

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
        db.save_project(&record)?;

        let desired_worktree_ids: std::collections::HashSet<String> = project
            .worktrees
            .iter()
            .enumerate()
            .map(|(index, _)| format!("{}-wt-{index}", project.id))
            .collect();
        for worktree in db.worktrees_of_project(&project.id)? {
            if !desired_worktree_ids.contains(&worktree.id) {
                db.remove_worktree(&worktree.id)?;
            }
        }
        for (worktree_index, worktree) in project.worktrees.iter().enumerate() {
            let mut record = WorktreeRecord::new(
                format!("{}-wt-{worktree_index}", project.id),
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
                diagnostics: vec![format!("project database unavailable: {error}")],
            };
        }
    };
    let mut projects = Vec::new();
    let mut diagnostics = Vec::new();
    for record in db.projects().unwrap_or_default() {
        let root = PathBuf::from(&record.root_path);
        if !root.is_dir() {
            diagnostics.push(format!("project {} vanished: {}", record.name, root.display()));
            continue;
        }
        match discover_project(&root) {
            Ok(discovered) => projects.push(catalog_project(&root, discovered)),
            Err(error) => diagnostics.push(format!("project {} could not be discovered: {error}", record.name)),
        }
    }
    RestoredCatalog { projects, diagnostics }
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
            return default_restored(fallback_directory);
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

    let mut tabs = Vec::new();
    for record in records {
        // Only the surfaces this shell can rebuild. Browser/editor/diff
        // tabs are skipped with a note, mirroring the Swift app's
        // `unavailableContent` diagnostic.
        if record.kind != "chat" && record.kind != "terminal" {
            diagnostics.push(format!(
                "tab {:?} ({}) is not restorable in this build; skipped",
                record.title, record.kind
            ));
            continue;
        }
        tabs.push(SessionTab {
            title: record.title,
            kind: record.kind,
            active: record.is_active,
        });
    }

    // The invariant is at most one active tab; if the stored layout has
    // none, the first tab is active, exactly like a fresh shell.
    if !tabs.is_empty() && !tabs.iter().any(|tab| tab.active) {
        tabs[0].active = true;
    }

    RestoredSession {
        working_directory,
        tabs,
        diagnostics,
    }
}

fn default_restored(fallback_directory: &Path) -> RestoredSession {
    RestoredSession {
        working_directory: fallback_directory.to_path_buf(),
        tabs: SessionLayout::default_in(fallback_directory.to_path_buf()).tabs,
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
        std::thread::spawn(move || loop {
            std::thread::sleep(FLUSH_POLL);
            let Some(inner) = inner.upgrade() else {
                return;
            };
            flush_if_due(&inner);
        });
    }

    /// Records the current layout; the next flush after the debounce
    /// interval writes it. Cheap: just swaps a snapshot.
    pub fn schedule(&self, layout: SessionLayout) {
        *self.inner.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(layout);
    }

    /// Persists the user's complete project catalog while preserving tabs on
    /// worktrees that are still present. Project discovery happens before this
    /// method is called, so the database write remains small and deterministic.
    pub fn schedule_catalog(&self, catalog: &ProjectCatalog) {
        let mut db = self.inner.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

    /// The number of real database writes performed so far.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn writes(&self) -> usize {
        self.inner.writes.load(Ordering::Relaxed)
    }

    /// Flushes the pending layout immediately (synchronously), ignoring the
    /// debounce window. Called on window close so quitting never loses the
    /// last change.
    pub fn flush_now(&self) {
        flush_if_due(&self.inner);
    }

    /// Whether the database is usable (false in fallback mode).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn persistence_enabled(&self) -> bool {
        self.inner.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some()
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

    fn layout(path: &Path, tabs: Vec<SessionTab>) -> SessionLayout {
        SessionLayout {
            working_directory: path.to_path_buf(),
            branch: "main".to_string(),
            tabs,
        }
    }

    fn three_tabs() -> Vec<SessionTab> {
        vec![
            SessionTab {
                title: "Chat".into(),
                kind: "chat".into(),
                active: false,
            },
            SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                active: false,
            },
            SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                active: true,
            },
        ]
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
        assert_eq!(restored.tabs, three_tabs(), "tabs, order and active flag come back");
        assert!(
            restored.diagnostics.is_empty(),
            "a healthy database restores without diagnostics"
        );
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
        assert!(catalog.projects()[0].worktrees.len() >= 1);
        assert!(!catalog.projects()[1].is_git);
        assert!(catalog.projects()[1].worktrees.is_empty());
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
        assert_eq!(restored.projects[0].root_path, surviving.canonicalize().unwrap());
        assert!(restored
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("vanished")));
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
        let installed = dir
            .0
            .join("Applications/Tiller.app/Contents/MacOS/tiller");
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
}

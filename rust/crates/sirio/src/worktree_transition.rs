use crate::session::{RestoredSession, SessionLayout, SessionStore, SessionTab};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

pub trait WorktreeTransitionStore: Send + Sync + 'static {
    fn save_layout(&self, layout: &SessionLayout);
    fn restore_tabs(&self, repo_root: &PathBuf) -> Result<RestoredSession, String>;
    fn load_session_refs(&self) -> BTreeMap<String, String>;
    fn secondary_pane_open(&self, repo_root: &PathBuf) -> bool;
}

pub struct SessionStoreTransitionStore(pub SessionStore);

impl WorktreeTransitionStore for SessionStoreTransitionStore {
    fn save_layout(&self, layout: &SessionLayout) {
        self.0.save_layout_now(layout);
    }

    fn restore_tabs(&self, repo_root: &PathBuf) -> Result<RestoredSession, String> {
        Ok(self.0.restore_tabs_for(repo_root))
    }

    fn load_session_refs(&self) -> BTreeMap<String, String> {
        self.0.load_session_refs()
    }

    fn secondary_pane_open(&self, repo_root: &PathBuf) -> bool {
        self.0.secondary_pane_open_for(repo_root)
    }
}

pub struct PersistedWorktreeSnapshot {
    pub layout: SessionLayout,
}

pub enum TransitionIntent {
    Boot { target: PathBuf },
    Switch {
        outgoing: PersistedWorktreeSnapshot,
        target: PathBuf,
    },
    LaunchSnapshot {
        target: PathBuf,
        launch_tabs: Vec<SessionTab>,
    },
}

pub struct ReadyWorktree {
    pub generation: u64,
    pub repo_root: PathBuf,
    pub restored: RestoredSession,
    pub session_refs: BTreeMap<String, String>,
    pub secondary_pane_open: bool,
    pub recovery: Option<String>,
}

pub enum TransitionVerdict {
    Ready(ReadyWorktree),
    Stale,
}

struct QueuedTransition {
    generation: u64,
    target: PathBuf,
    outgoing: Option<PersistedWorktreeSnapshot>,
}

struct TransitionState {
    next_generation: u64,
    pending: Option<QueuedTransition>,
    ready: VecDeque<TransitionVerdict>,
    shutdown: bool,
}

pub struct WorktreeTransition {
    state: Arc<(Mutex<TransitionState>, Condvar)>,
}

impl WorktreeTransition {
    pub fn new(store: Arc<dyn WorktreeTransitionStore>) -> Self {
        let state = Arc::new((
            Mutex::new(TransitionState {
                next_generation: 0,
                pending: None,
                ready: VecDeque::new(),
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_state = state.clone();
        let worker_store = store.clone();
        std::thread::spawn(move || worker(worker_store, worker_state));
        Self { state }
    }

    pub fn request(&self, intent: TransitionIntent) -> u64 {
        let (target, outgoing) = match intent {
            TransitionIntent::Boot { target } => (target, None),
            TransitionIntent::Switch { outgoing, target } => (target, Some(outgoing)),
            TransitionIntent::LaunchSnapshot {
                target,
                launch_tabs,
            } => {
                drop(launch_tabs);
                (target, None)
            }
        };
        let (state, wake) = &*self.state;
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.next_generation += 1;
        let generation = state.next_generation;
        state.pending = Some(QueuedTransition {
            generation,
            target,
            outgoing,
        });
        wake.notify_one();
        generation
    }

    pub fn take_ready(&self) -> impl Iterator<Item = TransitionVerdict> {
        let (state, _) = &*self.state;
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut state.ready).into_iter()
    }
}

impl Drop for WorktreeTransition {
    fn drop(&mut self) {
        let (state, wake) = &*self.state;
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.shutdown = true;
        wake.notify_one();
    }
}

fn worker(store: Arc<dyn WorktreeTransitionStore>, state: Arc<(Mutex<TransitionState>, Condvar)>) {
    loop {
        let queued = {
            let (lock, wake) = &*state;
            let mut guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            while guard.pending.is_none() && !guard.shutdown {
                guard = wake.wait(guard).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if guard.shutdown {
                return;
            }
            guard.pending.take().expect("pending transition exists")
        };

        if let Some(outgoing) = queued.outgoing.as_ref() {
            store.save_layout(&outgoing.layout);
        }
        let (restored, recovery) = match store.restore_tabs(&queued.target) {
            Ok(restored) => (restored, None),
            Err(error) => (
                {
                    let layout = SessionLayout::default_in(queued.target.clone());
                    RestoredSession {
                    working_directory: queued.target.clone(),
                    tabs: layout.tabs,
                    tab_states: layout.tab_states,
                    diagnostics: vec![format!("restore failed: {error}")],
                    }
                },
                Some(error),
            ),
        };
        let verdict = if restored.working_directory != queued.target {
            TransitionVerdict::Stale
        } else {
            let (lock, _) = &*state;
            let guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.next_generation != queued.generation {
                TransitionVerdict::Stale
            } else {
                TransitionVerdict::Ready(ReadyWorktree {
                    generation: queued.generation,
                    repo_root: queued.target.clone(),
                    restored,
                    session_refs: store.load_session_refs(),
                    secondary_pane_open: store.secondary_pane_open(&queued.target),
                    recovery,
                })
            }
        };
        let (lock, _) = &*state;
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ready
            .push_back(verdict);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    struct FakeStore {
        events: Mutex<Vec<String>>,
        block_restore: AtomicBool,
        release_restore: (Mutex<bool>, Condvar),
        fail_restore: AtomicBool,
    }

    impl FakeStore {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                block_restore: AtomicBool::new(false),
                release_restore: (Mutex::new(false), Condvar::new()),
                fail_restore: AtomicBool::new(false),
            }
        }

        fn release_restore(&self) {
            *self.release_restore.0.lock().unwrap() = true;
            self.release_restore.1.notify_all();
        }
    }

    impl WorktreeTransitionStore for FakeStore {
        fn save_layout(&self, layout: &SessionLayout) {
            self.events
                .lock()
                .unwrap()
                .push(format!("save:{}", layout.working_directory.display()));
        }

        fn restore_tabs(&self, repo_root: &PathBuf) -> Result<RestoredSession, String> {
            self.events
                .lock()
                .unwrap()
                .push(format!("load:{}", repo_root.display()));
            if self.block_restore.load(Ordering::SeqCst) {
                let mut released = self.release_restore.0.lock().unwrap();
                while !*released {
                    released = self.release_restore.1.wait(released).unwrap();
                }
            }
            if self.fail_restore.load(Ordering::SeqCst) {
                return Err("database unavailable".to_string());
            }
            Ok(RestoredSession {
                working_directory: repo_root.clone(),
                tabs: Vec::new(),
                tab_states: Vec::new(),
                diagnostics: Vec::new(),
            })
        }

        fn load_session_refs(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn secondary_pane_open(&self, _repo_root: &PathBuf) -> bool {
            false
        }
    }

    fn take_until_ready(coordinator: &WorktreeTransition) -> Vec<TransitionVerdict> {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let verdicts: Vec<_> = coordinator.take_ready().collect();
            if !verdicts.is_empty() || Instant::now() >= deadline {
                return verdicts;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn layout(path: &str) -> SessionLayout {
        SessionLayout::default_in(PathBuf::from(path))
    }

    #[test]
    fn request_does_not_block_while_restore_is_slow() {
        let store = Arc::new(FakeStore::new());
        store.block_restore.store(true, Ordering::SeqCst);
        let coordinator = WorktreeTransition::new(store.clone());
        coordinator.request(TransitionIntent::Boot {
            target: PathBuf::from("/slow"),
        });
        assert!(coordinator.take_ready().next().is_none());
        store.release_restore();
        assert!(matches!(take_until_ready(&coordinator).as_slice(), [TransitionVerdict::Ready(_)]));
    }

    #[test]
    fn failed_restore_returns_default_tabs_and_recovery() {
        let store = Arc::new(FakeStore::new());
        store.fail_restore.store(true, Ordering::SeqCst);
        let coordinator = WorktreeTransition::new(store);
        coordinator.request(TransitionIntent::Boot {
            target: PathBuf::from("/failed"),
        });
        let verdicts = take_until_ready(&coordinator);
        let [TransitionVerdict::Ready(ready)] = verdicts.as_slice() else {
            panic!("expected ready fallback");
        };
        assert_eq!(ready.restored.tabs.len(), 2);
        assert_eq!(ready.recovery.as_deref(), Some("database unavailable"));
    }

    #[test]
    fn rapid_requests_stale_the_earlier_completion() {
        let store = Arc::new(FakeStore::new());
        store.block_restore.store(true, Ordering::SeqCst);
        let coordinator = WorktreeTransition::new(store.clone());
        coordinator.request(TransitionIntent::Boot {
            target: PathBuf::from("/first"),
        });
        while store.events.lock().unwrap().is_empty() {
            std::thread::yield_now();
        }
        coordinator.request(TransitionIntent::Boot {
            target: PathBuf::from("/latest"),
        });
        store.release_restore();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut verdicts = Vec::new();
        while verdicts.len() < 2 && Instant::now() < deadline {
            verdicts.extend(coordinator.take_ready());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(verdicts[0], TransitionVerdict::Stale));
        assert!(matches!(&verdicts[1], TransitionVerdict::Ready(ready) if ready.repo_root == PathBuf::from("/latest")));
    }

    #[test]
    fn boot_and_switch_use_the_same_worker_path() {
        let store = Arc::new(FakeStore::new());
        let coordinator = WorktreeTransition::new(store.clone());
        coordinator.request(TransitionIntent::Boot {
            target: PathBuf::from("/boot"),
        });
        take_until_ready(&coordinator);
        coordinator.request(TransitionIntent::Switch {
            outgoing: PersistedWorktreeSnapshot {
                layout: layout("/boot"),
            },
            target: PathBuf::from("/selected"),
        });
        take_until_ready(&coordinator);
        assert_eq!(
            *store.events.lock().unwrap(),
            vec!["load:/boot", "save:/boot", "load:/selected"]
        );
    }

    #[test]
    fn switch_saves_before_loading_the_target() {
        let store = Arc::new(FakeStore::new());
        let coordinator = WorktreeTransition::new(store.clone());
        coordinator.request(TransitionIntent::Switch {
            outgoing: PersistedWorktreeSnapshot {
                layout: layout("/outgoing"),
            },
            target: PathBuf::from("/target"),
        });
        take_until_ready(&coordinator);
        assert_eq!(
            *store.events.lock().unwrap(),
            vec!["save:/outgoing", "load:/target"]
        );
    }
}

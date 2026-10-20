//! The file tab's drawer: the worktree's problems and when they call for it.
//!
//! `ProblemStore` keeps what each language server last said about each file;
//! `ErrorAlarm` decides, from the errors' identities, whether the drawer should
//! open by itself. Both are pure. `WorktreeDrawer` is the entity a file tab
//! draws under its code: open or closed, how tall, which tab. Search is the
//! worktree's query and results, `search_engine` run in the background and
//! `search_view` drawn.

mod alarm;
mod problems;
mod problems_view;
mod search_engine;
mod search_view;
mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use alarm::ErrorAlarm;
pub use problems::{ProblemCounts, ProblemStore, StoredProblem};
pub use search_engine::{
    DirtyHandle, FileResult, FoundHit, ReplaceReport, ReplaceSkip, SearchOutcome, SearchQuery,
    Skip, hits_in, rematch, replace_files, replace_in, search_worktree,
};
pub use search_view::{BufferSource, SearchFields, SearchRequest};
pub use view::{DrawerTab, WorktreeDrawer, WorktreeDrawerEvent};

/// A language server's identity, as `LspSupervisor` keys it: the project root
/// and the language name.
pub type ServerKey = (PathBuf, String);

/// An error's identity: the server that reported it, its file relative to the
/// worktree, and its message. The line is not part of it, so an edit that
/// shifts lines never makes an error new.
pub type ErrorKey = (ServerKey, PathBuf, String);

/// How many copies of each error identity are listed: two of one message in
/// one file count twice.
pub type ErrorCounts = BTreeMap<ErrorKey, usize>;

//! The file tab's drawer: the worktree's problems and when they call for it.
//!
//! This module holds the pure parts. `ProblemStore` keeps what each language
//! server last said about each file; `ErrorAlarm` decides, from the errors'
//! identities, whether the drawer should open by itself.

mod alarm;
mod problems;

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use alarm::ErrorAlarm;
pub use problems::{ProblemCounts, ProblemStore, StoredProblem};

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

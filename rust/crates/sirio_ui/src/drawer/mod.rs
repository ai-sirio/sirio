//! The file tab's drawer: the worktree's problems and when they call for it.
//!
//! This module holds the pure parts. `ProblemStore` keeps what each language
//! server last said about each file; `ErrorAlarm` decides, from the error
//! count alone, whether the drawer should open by itself.

mod alarm;
mod problems;

use std::path::PathBuf;

pub use alarm::{AlarmCause, ErrorAlarm};
pub use problems::{ProblemCounts, ProblemStore, StoredProblem};

/// A language server's identity, as `LspSupervisor` keys it: the project root
/// and the language name.
pub type ServerKey = (PathBuf, String);

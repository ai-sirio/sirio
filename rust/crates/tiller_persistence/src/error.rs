//! Errors produced by the persistence layer.
//!
//! Opening a corrupt or unreadable database is an error, never a panic, and
//! never a silent reset of the user's data: the file is left untouched and
//! [`PersistenceError::Corrupt`] tells the caller exactly what happened.

use std::fmt;
use std::path::PathBuf;

/// A failure while opening or using the database.
#[derive(Debug)]
pub enum PersistenceError {
    /// SQLite refused to read the file as a database (`SQLITE_NOTADB`) — the
    /// file exists but is corrupt or not a SQLite database. The file is left
    /// exactly as it was; the caller can back it up or surface the error.
    Corrupt {
        /// The path of the offending database file.
        path: PathBuf,
        /// The underlying SQLite message (e.g. "file is not a database").
        message: String,
    },
    /// The database was created by a newer version of the schema than this
    /// crate knows (`PRAGMA user_version` is higher than the highest
    /// migration). Opening it read-write could destroy data the crate does
    /// not understand, so it is refused.
    NewerSchema {
        /// The version recorded in the file.
        version: i64,
        /// The highest version this crate can read.
        supported: i64,
    },
    /// The database's logical page count is above the package's configured
    /// safety limit.
    DatabaseTooLarge {
        /// The path of the oversized database file.
        path: PathBuf,
        /// The logical size reported by SQLite (`page_count * page_size`).
        bytes: u64,
        /// The configured maximum logical size.
        max_bytes: u64,
    },
    /// The database is unreadable for another reason (permissions, missing
    /// file, disk error, SQL failure).
    Sqlite(rusqlite::Error),
    /// A filesystem operation failed.
    Io(std::io::Error),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::Corrupt { path, message } => {
                write!(f, "database at {} is corrupt: {message}", path.display())
            }
            PersistenceError::NewerSchema { version, supported } => write!(
                f,
                "database schema version {version} is newer than supported version {supported}"
            ),
            PersistenceError::DatabaseTooLarge {
                path,
                bytes,
                max_bytes,
            } => write!(
                f,
                "database at {} is {bytes} bytes, over the {max_bytes}-byte limit",
                path.display()
            ),
            PersistenceError::Sqlite(error) => write!(f, "sqlite error: {error}"),
            PersistenceError::Io(error) => write!(f, "io error: {error}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(error: rusqlite::Error) -> Self {
        PersistenceError::Sqlite(error)
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        PersistenceError::Io(error)
    }
}

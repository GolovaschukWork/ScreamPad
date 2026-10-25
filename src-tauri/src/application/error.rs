//! Errors shared between the application layer and its adapters.

use std::fmt;

/// Typed storage failure. Messages are generic on purpose: they never contain user file names or paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Another connection holds a lock longer than the busy timeout.
    Busy,
    /// The disk or the configured page limit is full.
    Full,
    /// The file is not a valid SQLite database or is damaged.
    Corrupt,
    /// The database was written by a newer version of the application.
    TooNew { database: usize, supported: usize },
    /// A migration failed; the whole upgrade was rolled back.
    Migration(String),
    /// A constraint (unique, foreign key, not null, check) was violated.
    Constraint,
    /// File system problem: missing directory, permissions, read-only media, I/O failure.
    Io(String),
    /// A previous holder of the writer lock panicked.
    Poisoned,
    /// Any other SQLite error.
    Sql(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => write!(f, "the database is busy"),
            Self::Full => write!(f, "the database or disk is full"),
            Self::Corrupt => write!(f, "the database file is not valid or is damaged"),
            Self::TooNew {
                database,
                supported,
            } => write!(
                f,
                "the database schema version {database} is newer than the supported version {supported}"
            ),
            Self::Migration(detail) => write!(f, "database migration failed: {detail}"),
            Self::Constraint => write!(f, "a database constraint was violated"),
            Self::Io(detail) => write!(f, "storage I/O error: {detail}"),
            Self::Poisoned => write!(f, "the database writer lock was poisoned"),
            Self::Sql(detail) => write!(f, "database error: {detail}"),
        }
    }
}

impl std::error::Error for StorageError {}

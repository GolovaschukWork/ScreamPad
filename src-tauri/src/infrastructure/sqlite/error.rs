use std::fmt;

use rusqlite::ErrorCode;
use rusqlite_migration::{Error as MigrationError, MigrationDefinitionError};

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

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::DiskFull) => Self::Full,
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => Self::Corrupt,
            Some(ErrorCode::ConstraintViolation) => Self::Constraint,
            Some(
                ErrorCode::CannotOpen
                | ErrorCode::ReadOnly
                | ErrorCode::PermissionDenied
                | ErrorCode::SystemIoFailure
                | ErrorCode::NotFound,
            ) => Self::Io(error.to_string()),
            _ => Self::Sql(error.to_string()),
        }
    }
}

impl From<MigrationError> for StorageError {
    fn from(error: MigrationError) -> Self {
        match error {
            MigrationError::RusqliteError { err, .. } => err.into(),
            MigrationError::MigrationDefinition(MigrationDefinitionError::DatabaseTooFarAhead) => {
                // The caller replaces the placeholder versions with the real ones.
                Self::TooNew {
                    database: 0,
                    supported: 0,
                }
            }
            other => Self::Migration(other.to_string()),
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind().to_string())
    }
}

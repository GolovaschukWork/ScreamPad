use rusqlite::ErrorCode;

use rusqlite_migration::{Error as MigrationError, MigrationDefinitionError};

use crate::application::error::StorageError;

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

use serde::Serialize;
use specta::Type;

use crate::infrastructure::sqlite::StorageError;

/// The error contract of the IPC layer. It carries a machine-readable `kind` and, where useful, numbers.
/// It never carries free text, file names or paths: the UI maps each `kind` to a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppError {
    StorageBusy,
    StorageFull,
    StorageCorrupt,
    #[serde(rename_all = "camelCase")]
    StorageTooNew {
        database: u32,
        supported: u32,
    },
    StorageMigration,
    StorageConstraint,
    StorageIo,
    Internal,
}

impl From<&StorageError> for AppError {
    fn from(error: &StorageError) -> Self {
        match error {
            StorageError::Busy => Self::StorageBusy,
            StorageError::Full => Self::StorageFull,
            StorageError::Corrupt => Self::StorageCorrupt,
            StorageError::TooNew {
                database,
                supported,
            } => Self::StorageTooNew {
                database: u32::try_from(*database).unwrap_or(u32::MAX),
                supported: u32::try_from(*supported).unwrap_or(u32::MAX),
            },
            StorageError::Migration(_) => Self::StorageMigration,
            StorageError::Constraint => Self::StorageConstraint,
            StorageError::Io(_) => Self::StorageIo,
            StorageError::Poisoned | StorageError::Sql(_) => Self::Internal,
        }
    }
}

impl From<StorageError> for AppError {
    fn from(error: StorageError) -> Self {
        Self::from(&error)
    }
}

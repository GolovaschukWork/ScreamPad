use serde::Serialize;
use specta::Type;

use crate::application::error::StorageError;
use crate::application::import::ImportError;

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
    ImportBusy,
    ImportCancelled,
    ImportSourceUnreadable,
    ImportSourceNotRegularFile,
    ImportSourceChanged,
    ImportEmpty,
    ImportTooLarge,
    ImportStagingQuotaExceeded,
    ImportDiskFull,
    ImportUnsupportedFormat,
    ImportCorrupt,
    ImportTooLong,
    ImportDecodeTimeout,
    LibraryInconsistent,
    /// The sound's managed file is gone. The database row is kept.
    MediaMissing,
    /// The sound's managed file does not match what was recorded.
    MediaCorrupt,
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

impl From<&ImportError> for AppError {
    fn from(error: &ImportError) -> Self {
        match error {
            ImportError::Busy => Self::ImportBusy,
            ImportError::Cancelled => Self::ImportCancelled,
            ImportError::SourceUnreadable => Self::ImportSourceUnreadable,
            ImportError::SourceNotRegularFile => Self::ImportSourceNotRegularFile,
            ImportError::SourceChanged => Self::ImportSourceChanged,
            ImportError::Empty => Self::ImportEmpty,
            ImportError::TooLarge => Self::ImportTooLarge,
            ImportError::StagingQuotaExceeded => Self::ImportStagingQuotaExceeded,
            ImportError::DiskFull => Self::ImportDiskFull,
            ImportError::UnsupportedFormat => Self::ImportUnsupportedFormat,
            ImportError::Corrupt => Self::ImportCorrupt,
            ImportError::TooLong => Self::ImportTooLong,
            ImportError::DecodeTimeout => Self::ImportDecodeTimeout,
            ImportError::LibraryInconsistent => Self::LibraryInconsistent,
            ImportError::Storage(storage) => Self::from(storage),
            ImportError::Io(_) => Self::StorageIo,
        }
    }
}

impl From<ImportError> for AppError {
    fn from(error: ImportError) -> Self {
        Self::from(&error)
    }
}

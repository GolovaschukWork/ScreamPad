use std::path::Path;
use std::sync::Arc;

use crate::commands::error::AppError;
use crate::infrastructure::sqlite::{Database, migrations};

/// File name of the library database inside the application data directory.
const DATABASE_FILE: &str = "library.db";

/// Result of opening the library at startup. A failure is kept and reported to the UI instead of aborting the app,
/// and a damaged database is never modified or deleted automatically.
pub enum StorageState {
    Ready(Arc<Database>),
    Unavailable(AppError),
}

impl StorageState {
    pub fn open(data_dir: &Path) -> Self {
        match Database::open(&data_dir.join(DATABASE_FILE), &migrations()) {
            Ok(database) => Self::Ready(Arc::new(database)),
            Err(error) => Self::Unavailable(AppError::from(error)),
        }
    }

    pub fn database(&self) -> Result<Arc<Database>, AppError> {
        match self {
            Self::Ready(database) => Ok(Arc::clone(database)),
            Self::Unavailable(error) => Err(error.clone()),
        }
    }
}

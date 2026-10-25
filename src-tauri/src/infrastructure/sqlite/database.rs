use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use rusqlite_migration::Migrations;

use super::migrations::{BackupTarget, MigrationOutcome, migrate};
use super::{StorageError, connection};

const BACKUPS_TO_KEEP: usize = 3;

/// The library database. The writer is serialized behind a mutex; call it from a blocking task, never from the audio thread.
pub struct Database {
    path: PathBuf,
    writer: Mutex<Connection>,
    outcome: MigrationOutcome,
}

impl Database {
    /// Opens (creating if needed) and migrates the database at `path`.
    pub fn open(path: &Path, migrations: &Migrations<'_>) -> Result<Self, StorageError> {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&dir)?;
        }
        let mut writer = connection::open_writer(path)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("library")
            .to_owned();
        let backup = BackupTarget {
            dir,
            stem,
            keep: BACKUPS_TO_KEEP,
        };
        let outcome = migrate(&mut writer, migrations, Some(&backup))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer: Mutex::new(writer),
            outcome,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the startup migration did.
    pub fn migration_outcome(&self) -> &MigrationOutcome {
        &self.outcome
    }

    /// Runs `work` with the writer connection.
    pub fn with_writer<T>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut guard = self.writer.lock().map_err(|_| StorageError::Poisoned)?;
        work(&mut guard)
    }

    /// Opens a fresh read-only connection. A fixed pool is added when the first read path needs it.
    pub fn reader(&self) -> Result<Connection, StorageError> {
        connection::open_reader(&self.path)
    }
}

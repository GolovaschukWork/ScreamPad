use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::StorageError;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Opens the single writer connection with the settings from ADR 0005.
pub fn open_writer(path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open(path)?;
    configure(&connection, true)?;
    Ok(connection)
}

/// Opens a read-only connection. In WAL mode readers do not wait for the writer.
pub fn open_reader(path: &Path) -> Result<Connection, StorageError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let connection = Connection::open_with_flags(path, flags)?;
    configure(&connection, false)?;
    Ok(connection)
}

fn configure(connection: &Connection, writable: bool) -> Result<(), StorageError> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    // The first statement reads the file header, so a damaged file is reported here as `Corrupt`.
    if writable {
        connection.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
    } else {
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    }
    Ok(())
}

/// Renames a damaged database (and its WAL/SHM side files) out of the way. Nothing is ever deleted.
/// Returns the new location of the main file so the UI can point the user to it.
pub fn quarantine_corrupt(path: &Path) -> Result<PathBuf, StorageError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let target = sibling_with_suffix(path, &format!(".corrupt-{seconds}"));
    std::fs::rename(path, &target)?;
    for side in ["-wal", "-shm"] {
        let from = sibling_with_suffix(path, side);
        if from.exists() {
            std::fs::rename(&from, sibling_with_suffix(&target, side))?;
        }
    }
    Ok(target)
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

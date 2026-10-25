use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use super::StorageError;

/// One shipped, forward-only migration. Files live in `src-tauri/migrations/NNNN_description.sql`.
pub struct ShippedMigration {
    pub name: &'static str,
    pub sql: &'static str,
}

/// Every migration that has been released, in order. Adding one needs owner approval (schema change).
pub const MIGRATIONS: &[ShippedMigration] = &[ShippedMigration {
    name: "0001_create_sounds",
    sql: include_str!("../../../migrations/0001_create_sounds.sql"),
}];

/// FNV-1a checksums recorded when each migration shipped, same order as `MIGRATIONS`.
/// The library stores no checksums, so editing a released migration is caught by the tests instead.
pub const SHIPPED_CHECKSUMS: &[(&str, u64)] = &[("0001_create_sounds", 0x6f9d724e549f738c)];

/// Non-cryptographic checksum, enough to detect an accidental edit without adding a dependency.
pub fn checksum(sql: &str) -> u64 {
    sql.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The production migration list.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(
        MIGRATIONS
            .iter()
            .map(|m| M::up(m.sql).foreign_key_check())
            .collect(),
    )
}

/// Where and how pre-upgrade backups are kept.
pub struct BackupTarget {
    pub dir: PathBuf,
    pub stem: String,
    pub keep: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub from: usize,
    pub to: usize,
    pub backup: Option<PathBuf>,
}

/// Brings the database to the latest schema.
///
/// - A database newer than this build is rejected untouched (`TooNew`).
/// - An existing database is backed up with `VACUUM INTO` before pending migrations run, and a failed backup aborts the upgrade.
/// - The upgrade is atomic: when any migration fails, none of the pending ones is kept.
pub fn migrate(
    connection: &mut Connection,
    migrations: &Migrations<'_>,
    backup: Option<&BackupTarget>,
) -> Result<MigrationOutcome, StorageError> {
    let from = usize::from(migrations.current_version(connection)?);
    let pending = migrations.pending_migrations(connection)?;
    if pending < 0 {
        let supported =
            usize::try_from(i64::try_from(from).unwrap_or(i64::MAX) + i64::from(pending))
                .unwrap_or(0);
        return Err(StorageError::TooNew {
            database: from,
            supported,
        });
    }
    let to = from + usize::try_from(pending).unwrap_or(0);
    if pending == 0 {
        return Ok(MigrationOutcome {
            from,
            to,
            backup: None,
        });
    }

    let backup_path = match backup {
        Some(target) if from > 0 => Some(create_backup(connection, target, from, to)?),
        _ => None,
    };
    migrations.to_latest(connection)?;
    Ok(MigrationOutcome {
        from,
        to,
        backup: backup_path,
    })
}

fn create_backup(
    connection: &Connection,
    target: &BackupTarget,
    from: usize,
    to: usize,
) -> Result<PathBuf, StorageError> {
    fs::create_dir_all(&target.dir)?;
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = target
        .dir
        .join(format!("{}-pre-v{from}-to-v{to}-{seconds}.db", target.stem));
    let destination = path
        .to_str()
        .ok_or_else(|| StorageError::Io("backup path is not valid UTF-8".to_owned()))?;
    connection.execute("VACUUM INTO ?1", [destination])?;
    prune_backups(target)?;
    Ok(path)
}

/// Keeps the newest `keep` backups, ordered by the schema version they were taken for.
fn prune_backups(target: &BackupTarget) -> Result<(), StorageError> {
    let prefix = format!("{}-pre-v", target.stem);
    let mut backups: Vec<(usize, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&target.dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(version) = backup_target_version(name, &prefix) {
            backups.push((version, path));
        }
    }
    backups.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, stale) in backups.into_iter().skip(target.keep) {
        fs::remove_file(stale)?;
    }
    Ok(())
}

fn backup_target_version(file_name: &str, prefix: &str) -> Option<usize> {
    let rest = file_name.strip_prefix(prefix)?.strip_suffix(".db")?;
    let (_, after_to) = rest.split_once("-to-v")?;
    after_to.split('-').next()?.parse().ok()
}

#[cfg(test)]
pub(super) fn list_backups(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let prefix = format!("{stem}-pre-v");
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| backup_target_version(n, &prefix).is_some())
        })
        .collect();
    found.sort();
    found
}

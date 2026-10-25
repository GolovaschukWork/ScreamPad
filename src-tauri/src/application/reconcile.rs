//! Startup reconciliation between the database and the managed files (ADR 0007).
//!
//! It runs in the background and never blocks the UI. It only deletes files this application created and that nothing
//! refers to: leftovers in `staging/` that no running import owns, and orphans in `media/` that no row refers to, that no
//! running import is publishing, and that are older than the grace period. It never deletes a referenced file and never
//! deletes or changes a database row: a row whose file is gone is only reported.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use super::active::ActiveImports;
use super::error::StorageError;
use super::ports::{MediaFs, SoundRepository};
use crate::domain::sound::StorageKey;

/// Orphans younger than this are kept: they may belong to an import that is about to be repeated or adopted.
pub const ORPHAN_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    pub staging_removed: u32,
    pub orphans_removed: u32,
    pub orphans_within_grace: u32,
    /// Rows whose managed file is missing.
    pub missing: Vec<i64>,
    /// Rows whose managed file has a different size than recorded.
    pub size_mismatch: Vec<i64>,
    /// Files that could not be inspected or removed; they are retried on the next start.
    pub skipped: u32,
}

pub fn reconcile(
    fs: &dyn MediaFs,
    repo: &dyn SoundRepository,
    active: &ActiveImports,
    now: SystemTime,
    grace: Duration,
) -> Result<ReconcileReport, StorageError> {
    let mut report = ReconcileReport::default();

    // 1. Staging leftovers that no running import owns.
    match fs.staging_entries() {
        Ok(entries) => {
            for entry in entries {
                if active.is_staging_active(&entry.name) {
                    continue;
                }
                match fs.remove_staged(&entry.name) {
                    Ok(()) => report.staging_removed += 1,
                    Err(_) => report.skipped += 1,
                }
            }
        }
        Err(_) => report.skipped += 1,
    }

    // 2. Rows versus files.
    let refs = repo.media_refs()?;
    let referenced: HashSet<&str> = refs.iter().map(|r| r.key.as_str()).collect();
    for reference in &refs {
        match fs.media_size(&reference.key) {
            Ok(None) => report.missing.push(reference.id),
            Ok(Some(size)) if size != reference.size_bytes => {
                report.size_mismatch.push(reference.id)
            }
            Ok(Some(_)) => {}
            Err(_) => report.skipped += 1,
        }
    }

    // 3. Orphan files.
    match fs.media_entries() {
        Ok(entries) => {
            for entry in entries {
                // Names that are not valid storage keys are not ours; leave them alone.
                let Ok(key) = StorageKey::parse(&entry.key) else {
                    continue;
                };
                if referenced.contains(key.as_str()) || active.is_key_active(key.as_str()) {
                    continue;
                }
                let old_enough = entry
                    .modified
                    .and_then(|modified| now.duration_since(modified).ok())
                    .is_some_and(|age| age >= grace);
                if !old_enough {
                    report.orphans_within_grace += 1;
                    continue;
                }
                // Re-check right before deleting: an import may have committed since the snapshot above.
                if active.is_key_active(key.as_str()) || repo.key_exists(&key)? {
                    continue;
                }
                match fs.remove_media(&key) {
                    Ok(()) => report.orphans_removed += 1,
                    Err(_) => report.skipped += 1,
                }
            }
        }
        Err(_) => report.skipped += 1,
    }
    Ok(report)
}

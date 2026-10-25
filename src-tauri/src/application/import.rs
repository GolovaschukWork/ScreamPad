//! Import of one local audio file into the managed library (ADR 0007).
//!
//! Order: validate the source, copy and hash into `staging/`, inspect by content, check for a duplicate, durably publish
//! into `media/`, then commit the row. The file is durable before the commit, so a committed row never points at a file
//! that a power failure could take away. Everything before the publish step is rolled back by deleting the staged file.

use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::active::ActiveImports;
use super::cancel::CancelToken;
use super::error::StorageError;
use super::ports::{
    AudioProbe, Clock, InsertOutcome, MediaFs, ProbeError, ProbeLimits, SoundRepository,
};
use crate::domain::sound::{
    ContentHash, NewSound, Sound, StorageKey, display_name_from_file_name, sanitize_file_name,
};
use crate::infrastructure::media::hash::StreamHasher;

/// Read and write chunk. Memory use is constant regardless of the file size.
const CHUNK_SIZE: usize = 64 * 1024;

/// Resource limits for one import. The values are **provisional**: the owner will set the real ones from measurements
/// before beta. They exist so that no import is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    /// Largest accepted source, also enforced while copying in case the file grows.
    pub max_file_bytes: u64,
    /// Largest total size of everything in `staging/`, including this import.
    pub max_staging_bytes: u64,
    /// Longest accepted audio, measured by decoding rather than trusted from headers.
    pub max_duration_ms: u64,
    /// Wall-clock budget for decoding a file during validation.
    pub max_decode_time: Duration,
}

impl ImportLimits {
    pub const fn provisional() -> Self {
        Self {
            max_file_bytes: 512 * 1024 * 1024,
            max_staging_bytes: 1024 * 1024 * 1024,
            max_duration_ms: 30 * 60 * 1000,
            max_decode_time: Duration::from_secs(120),
        }
    }
}

/// Why an import failed. Never carries paths or file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// Another import is running; imports are serialized.
    Busy,
    Cancelled,
    SourceUnreadable,
    SourceNotRegularFile,
    /// The source changed while it was being copied.
    SourceChanged,
    Empty,
    TooLarge,
    StagingQuotaExceeded,
    DiskFull,
    UnsupportedFormat,
    /// Recognized but truncated, corrupt or otherwise undecodable.
    Corrupt,
    TooLong,
    DecodeTimeout,
    /// A different file already occupies the target key in the library.
    LibraryInconsistent,
    Storage(StorageError),
    Io(io::ErrorKind),
}

impl From<StorageError> for ImportError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "import failed: {self:?}")
    }
}

impl std::error::Error for ImportError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Imported(Sound),
    /// The same bytes were already in the library; nothing was added.
    AlreadyInLibrary(Sound),
}

pub struct ImportService {
    fs: Arc<dyn MediaFs>,
    probe: Arc<dyn AudioProbe>,
    repo: Arc<dyn SoundRepository>,
    clock: Arc<dyn Clock>,
    active: Arc<ActiveImports>,
    limits: ImportLimits,
    /// Imports are serialized; a second caller gets `Busy` instead of queueing without bound.
    running: Mutex<()>,
    counter: AtomicU64,
}

impl ImportService {
    pub fn new(
        fs: Arc<dyn MediaFs>,
        probe: Arc<dyn AudioProbe>,
        repo: Arc<dyn SoundRepository>,
        clock: Arc<dyn Clock>,
        active: Arc<ActiveImports>,
        limits: ImportLimits,
    ) -> Self {
        Self {
            fs,
            probe,
            repo,
            clock,
            active,
            limits,
            running: Mutex::new(()),
            counter: AtomicU64::new(0),
        }
    }

    /// Imports the file at `source`. Safe to call from any thread; blocks on file I/O, so call it from a blocking task.
    pub fn import(
        &self,
        source: &Path,
        cancel: &CancelToken,
    ) -> Result<ImportOutcome, ImportError> {
        let _running = match self.running.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Err(ImportError::Busy),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }

        let mut handle = self.fs.open_source(source).map_err(map_open_error)?;
        let before = handle.info().map_err(map_source_error)?;
        if !before.is_regular_file {
            return Err(ImportError::SourceNotRegularFile);
        }
        if before.size == 0 {
            return Err(ImportError::Empty);
        }
        if before.size > self.limits.max_file_bytes {
            return Err(ImportError::TooLarge);
        }
        let staged_bytes: u64 = self
            .fs
            .staging_entries()
            .map_err(map_io_error)?
            .iter()
            .map(|entry| entry.size)
            .sum();
        if staged_bytes.saturating_add(before.size) > self.limits.max_staging_bytes {
            return Err(ImportError::StagingQuotaExceeded);
        }

        let name = self.staging_name();
        // Registered before the file exists, so reconciliation can never delete it.
        let _staging_guard = self.active.register_staging(&name);
        let result = self.import_staged(&name, source, &mut *handle, &before, cancel);
        // Rollback and cleanup: everything before the publish step lives only in the staged file, and a duplicate leaves its
        // staged copy behind. After a successful publish the staged name no longer exists, so this is then a no-op.
        let _ = self.fs.remove_staged(&name);
        result
    }

    fn import_staged(
        &self,
        name: &str,
        source: &Path,
        handle: &mut dyn crate::application::ports::SourceHandle,
        before: &crate::application::ports::SourceInfo,
        cancel: &CancelToken,
    ) -> Result<ImportOutcome, ImportError> {
        // 1. Copy and hash in a single pass, so the hash describes exactly the bytes that were stored.
        let mut staged = self.fs.create_staging(name).map_err(map_io_error)?;
        let mut hasher = StreamHasher::new();
        let mut buffer = vec![0_u8; CHUNK_SIZE];
        let mut total: u64 = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(ImportError::Cancelled);
            }
            let read = match handle.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(map_source_error(error)),
            };
            total += read as u64;
            if total > self.limits.max_file_bytes {
                return Err(ImportError::TooLarge);
            }
            hasher.update(&buffer[..read]);
            staged.write_all(&buffer[..read]).map_err(map_io_error)?;
        }
        staged.sync_all().map_err(map_io_error)?;
        drop(staged);

        let after = handle.info().map_err(map_source_error)?;
        if total != before.size || after.size != before.size || after.modified != before.modified {
            return Err(ImportError::SourceChanged);
        }
        let hash = ContentHash::parse(&hasher.finish())
            .map_err(|_| ImportError::Io(io::ErrorKind::InvalidData))?;

        // 2. Inspect the staged copy by content. A panic inside a third-party decoder is contained and reported as corrupt.
        let file = self.fs.open_staged(name).map_err(map_io_error)?;
        let limits = ProbeLimits {
            max_duration_ms: self.limits.max_duration_ms,
            max_decode_time: self.limits.max_decode_time,
        };
        let audio = match catch_unwind(AssertUnwindSafe(|| self.probe.probe(file, &limits, cancel)))
        {
            Ok(Ok(audio)) => audio,
            Ok(Err(error)) => return Err(map_probe_error(error)),
            Err(_) => return Err(ImportError::Corrupt),
        };

        // 3. Duplicate handling: the same bytes are never stored twice.
        if let Some(existing) = self.repo.find_by_hash(&hash)? {
            return Ok(ImportOutcome::AlreadyInLibrary(existing));
        }

        // No cancellation past this point: publish and commit are the commit sequence.
        if cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        let key = StorageKey::new(&hash, audio.format);
        let _key_guard = self.active.register_key(key.as_str());

        // 4. Durable publish, then 5. the database commit. A failure after the publish leaves an orphan file that the next
        // import of the same bytes adopts and that reconciliation removes after the grace period; it is never deleted here
        // because the row may or may not exist.
        self.fs.publish(name, &key).map_err(map_publish_error)?;

        let sanitized = sanitize_file_name(
            &source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let new_sound = NewSound {
            content_hash: hash,
            display_name: display_name_from_file_name(sanitized.as_deref()),
            original_file_name: sanitized,
            key,
            format: audio.format,
            duration_ms: audio.duration_ms,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            size_bytes: total,
            created_at_ms: self.clock.now_ms(),
        };
        match self.repo.insert(&new_sound)? {
            InsertOutcome::Inserted(sound) => Ok(ImportOutcome::Imported(sound)),
            InsertOutcome::Duplicate(existing) => Ok(ImportOutcome::AlreadyInLibrary(existing)),
        }
    }

    fn staging_name(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{:x}-{nanos:x}-{count:x}.part", std::process::id())
    }
}

/// Errors from opening the source: a path that is not a regular file is reported as such.
fn map_open_error(error: io::Error) -> ImportError {
    match error.kind() {
        io::ErrorKind::InvalidInput => ImportError::SourceNotRegularFile,
        _ => map_source_error(error),
    }
}

fn map_source_error(error: io::Error) -> ImportError {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => ImportError::SourceUnreadable,
        other => ImportError::Io(other),
    }
}

fn map_io_error(error: io::Error) -> ImportError {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => ImportError::DiskFull,
        other => ImportError::Io(other),
    }
}

fn map_publish_error(error: io::Error) -> ImportError {
    match error.kind() {
        io::ErrorKind::AlreadyExists => ImportError::LibraryInconsistent,
        _ => map_io_error(error),
    }
}

fn map_probe_error(error: ProbeError) -> ImportError {
    match error {
        ProbeError::Unsupported => ImportError::UnsupportedFormat,
        ProbeError::Invalid => ImportError::Corrupt,
        ProbeError::TooLong => ImportError::TooLong,
        ProbeError::TimedOut => ImportError::DecodeTimeout,
        ProbeError::Cancelled => ImportError::Cancelled,
        ProbeError::Io(kind) => map_io_error(io::Error::from(kind)),
    }
}

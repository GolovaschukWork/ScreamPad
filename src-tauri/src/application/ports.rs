//! Ports implemented by the infrastructure layer. Tests replace or wrap them to inject failures.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::cancel::CancelToken;
use super::error::StorageError;
use crate::domain::sound::{AudioFormat, ContentHash, NewSound, Sound, StorageKey};

/// What the application knows about a file it is about to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_regular_file: bool,
}

/// A user-selected source opened exactly once. `info` inspects the opened handle, not the path again.
pub trait SourceHandle: Read + Send {
    fn info(&self) -> io::Result<SourceInfo>;
}

/// A staging file being written.
pub trait StagedFile: Write + Send {
    /// Flushes data and metadata to stable storage.
    fn sync_all(&mut self) -> io::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEntry {
    /// `<shard>/<file name>` exactly as found on disk; not yet validated as a storage key.
    pub key: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Published {
    /// The staged file was moved into the library.
    Moved,
    /// A byte-identical file already existed (an orphan of an earlier interrupted import); the staged copy was discarded.
    AdoptedExisting,
}

/// File-system operations of the managed library: a `staging/` and a `media/` directory on the same volume.
pub trait MediaFs: Send + Sync {
    fn open_source(&self, path: &Path) -> io::Result<Box<dyn SourceHandle>>;
    /// Creates a new, empty staging file; fails if the name exists.
    fn create_staging(&self, name: &str) -> io::Result<Box<dyn StagedFile>>;
    fn open_staged(&self, name: &str) -> io::Result<File>;
    /// Removes a staging file; a missing file is not an error.
    fn remove_staged(&self, name: &str) -> io::Result<()>;
    fn staging_entries(&self) -> io::Result<Vec<FileEntry>>;
    /// Durably moves a staged file to its storage key and makes it read-only. A different file at the key is
    /// reported as `ErrorKind::AlreadyExists`.
    fn publish(&self, name: &str, key: &StorageKey) -> io::Result<Published>;
    fn media_entries(&self) -> io::Result<Vec<MediaEntry>>;
    /// Size of the managed file, or `None` when it is missing or is not a regular file.
    fn media_size(&self, key: &StorageKey) -> io::Result<Option<u64>>;
    /// Removes a managed file (clearing its read-only flag first). A missing file is not an error.
    fn remove_media(&self, key: &StorageKey) -> io::Result<()>;
}

/// What inspecting an audio file by content found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioInfo {
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeLimits {
    pub max_duration_ms: u64,
    pub max_decode_time: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// Not a recognized, supported audio file.
    Unsupported,
    /// Recognized but empty, truncated or corrupt.
    Invalid,
    TooLong,
    TimedOut,
    Cancelled,
    Io(io::ErrorKind),
}

/// Identifies the format from the file's content (never from its name) and decodes it to verify it.
pub trait AudioProbe: Send + Sync {
    fn probe(
        &self,
        file: File,
        limits: &ProbeLimits,
        cancel: &CancelToken,
    ) -> Result<AudioInfo, ProbeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted(Sound),
    /// A sound with the same content hash already existed.
    Duplicate(Sound),
}

/// A row's reference to a managed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRef {
    pub id: i64,
    pub key: StorageKey,
    pub size_bytes: u64,
}

pub trait SoundRepository: Send + Sync {
    fn find_by_hash(&self, hash: &ContentHash) -> Result<Option<Sound>, StorageError>;
    /// Inserts atomically; an existing row with the same hash is returned as `Duplicate`.
    fn insert(&self, sound: &NewSound) -> Result<InsertOutcome, StorageError>;
    fn key_exists(&self, key: &StorageKey) -> Result<bool, StorageError>;
    fn media_refs(&self) -> Result<Vec<MediaRef>, StorageError>;
    fn list(&self) -> Result<Vec<Sound>, StorageError>;
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// Lets the user choose one audio file. Implemented by the native dialog; the webview never supplies a path.
pub trait FilePicker: Send + Sync {
    /// Blocks until the user chooses a file or cancels (`None`). Call it from a blocking task.
    fn pick_audio_file(&self) -> Option<PathBuf>;
}

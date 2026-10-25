//! Test harness for the managed library: real adapters on a temporary directory, wrapped so tests can inject faults at every
//! step of an import (ADR 0007). Nothing here is compiled into the application.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::active::ActiveImports;
use super::cancel::CancelToken;
use super::error::StorageError;
use super::import::{ImportError, ImportLimits, ImportOutcome, ImportService};
use super::ports::{
    AudioInfo, AudioProbe, Clock, FileEntry, InsertOutcome, MediaEntry, MediaFs, MediaRef,
    ProbeError, ProbeLimits, Published, SoundRepository, SourceHandle, SourceInfo, StagedFile,
};
use super::reconcile::{ReconcileReport, reconcile};
use crate::domain::sound::{ContentHash, NewSound, Sound, StorageKey};
use crate::infrastructure::media::fs::OsMediaFs;
use crate::infrastructure::media::hash::sha256_hex_file;
use crate::infrastructure::media::probe::SymphoniaProbe;
use crate::infrastructure::sqlite::{Database, SqliteSoundRepository, migrations};
use crate::test_support::TempDir;

/// Faults a test can arm. Each is consumed or checked at one specific step.
#[derive(Default)]
pub struct Faults {
    pub open_source: Mutex<Option<io::ErrorKind>>,
    /// Writes to the staging file fail once this many bytes were written.
    pub write_after: Mutex<Option<(u64, io::ErrorKind)>>,
    /// Reads from the source fail once this many bytes were read.
    pub read_after: Mutex<Option<(u64, io::ErrorKind)>>,
    /// Cancels this token when the first read happens.
    pub cancel_on_first_read: Mutex<Option<CancelToken>>,
    /// The source reports a different size the second time it is inspected.
    pub source_changes: AtomicBool,
    pub sync_staging: Mutex<Option<io::ErrorKind>>,
    /// `publish` fails before touching any file.
    pub publish: Mutex<Option<io::ErrorKind>>,
    /// The next insert fails as if the commit did not happen.
    pub insert: AtomicBool,
}

pub struct FaultyFs {
    pub inner: OsMediaFs,
    pub faults: Arc<Faults>,
}

struct FaultySource {
    inner: Box<dyn SourceHandle>,
    faults: Arc<Faults>,
    read: u64,
    inspections: u32,
    first_read: bool,
}

impl Read for FaultySource {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.first_read {
            self.first_read = false;
            if let Some(token) = self
                .faults
                .cancel_on_first_read
                .lock()
                .expect("faults")
                .as_ref()
            {
                token.cancel();
            }
        }
        let mut allowed = buffer.len();
        if let Some((limit, kind)) = *self.faults.read_after.lock().expect("faults") {
            if self.read >= limit {
                return Err(io::Error::from(kind));
            }
            // Deliver only the bytes before the fault point, so the failure happens in the middle of the file.
            allowed = allowed.min(usize::try_from(limit - self.read).unwrap_or(usize::MAX));
        }
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.read += read as u64;
        Ok(read)
    }
}

impl SourceHandle for FaultySource {
    fn info(&self) -> io::Result<SourceInfo> {
        let mut info = self.inner.info()?;
        if self.faults.source_changes.load(Ordering::Relaxed) && self.read > 0 {
            info.size += 1;
        }
        let _ = self.inspections;
        Ok(info)
    }
}

struct FaultyStaged {
    inner: Box<dyn StagedFile>,
    faults: Arc<Faults>,
    written: u64,
}

impl Write for FaultyStaged {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut allowed = buffer.len();
        if let Some((limit, kind)) = *self.faults.write_after.lock().expect("faults") {
            if self.written >= limit {
                return Err(io::Error::from(kind));
            }
            allowed = allowed.min(usize::try_from(limit - self.written).unwrap_or(usize::MAX));
        }
        let written = self.inner.write(&buffer[..allowed])?;
        self.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl StagedFile for FaultyStaged {
    fn sync_all(&mut self) -> io::Result<()> {
        if let Some(kind) = *self.faults.sync_staging.lock().expect("faults") {
            return Err(io::Error::from(kind));
        }
        self.inner.sync_all()
    }
}

impl MediaFs for FaultyFs {
    fn open_source(&self, path: &Path) -> io::Result<Box<dyn SourceHandle>> {
        if let Some(kind) = *self.faults.open_source.lock().expect("faults") {
            return Err(io::Error::from(kind));
        }
        Ok(Box::new(FaultySource {
            inner: self.inner.open_source(path)?,
            faults: Arc::clone(&self.faults),
            read: 0,
            inspections: 0,
            first_read: true,
        }))
    }

    fn create_staging(&self, name: &str) -> io::Result<Box<dyn StagedFile>> {
        Ok(Box::new(FaultyStaged {
            inner: self.inner.create_staging(name)?,
            faults: Arc::clone(&self.faults),
            written: 0,
        }))
    }

    fn open_staged(&self, name: &str) -> io::Result<File> {
        self.inner.open_staged(name)
    }

    fn remove_staged(&self, name: &str) -> io::Result<()> {
        self.inner.remove_staged(name)
    }

    fn staging_entries(&self) -> io::Result<Vec<FileEntry>> {
        self.inner.staging_entries()
    }

    fn publish(&self, name: &str, key: &StorageKey) -> io::Result<Published> {
        if let Some(kind) = *self.faults.publish.lock().expect("faults") {
            return Err(io::Error::from(kind));
        }
        self.inner.publish(name, key)
    }

    fn media_entries(&self) -> io::Result<Vec<MediaEntry>> {
        self.inner.media_entries()
    }

    fn media_size(&self, key: &StorageKey) -> io::Result<Option<u64>> {
        self.inner.media_size(key)
    }

    fn remove_media(&self, key: &StorageKey) -> io::Result<()> {
        self.inner.remove_media(key)
    }

    fn open_media(&self, key: &StorageKey) -> io::Result<File> {
        self.inner.open_media(key)
    }
}

pub struct FaultyRepo {
    pub inner: SqliteSoundRepository,
    pub faults: Arc<Faults>,
}

impl SoundRepository for FaultyRepo {
    fn find_by_id(&self, id: i64) -> Result<Option<Sound>, StorageError> {
        self.inner.find_by_id(id)
    }

    fn find_by_hash(&self, hash: &ContentHash) -> Result<Option<Sound>, StorageError> {
        self.inner.find_by_hash(hash)
    }

    fn insert(&self, sound: &NewSound) -> Result<InsertOutcome, StorageError> {
        if self.faults.insert.swap(false, Ordering::Relaxed) {
            return Err(StorageError::Busy);
        }
        self.inner.insert(sound)
    }

    fn key_exists(&self, key: &StorageKey) -> Result<bool, StorageError> {
        self.inner.key_exists(key)
    }

    fn media_refs(&self) -> Result<Vec<MediaRef>, StorageError> {
        self.inner.media_refs()
    }

    fn list(&self) -> Result<Vec<Sound>, StorageError> {
        self.inner.list()
    }
}

/// A decoder that crashes, like a bug triggered by a hostile file.
pub struct PanickingProbe;

impl AudioProbe for PanickingProbe {
    fn probe(
        &self,
        _file: File,
        _limits: &ProbeLimits,
        _cancel: &CancelToken,
    ) -> Result<AudioInfo, ProbeError> {
        panic!("simulated decoder bug");
    }
}

/// Lets a test hold an import inside the probe step and release it later.
pub struct GateProbe {
    pub inner: SymphoniaProbe,
    entered: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

pub struct Gate {
    pub entered: Receiver<()>,
    pub release: Sender<()>,
}

impl GateProbe {
    pub fn new() -> (Self, Gate) {
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let probe = Self {
            inner: SymphoniaProbe,
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release_rx),
        };
        (
            probe,
            Gate {
                entered: entered_rx,
                release: release_tx,
            },
        )
    }
}

impl AudioProbe for GateProbe {
    fn probe(
        &self,
        file: File,
        limits: &ProbeLimits,
        cancel: &CancelToken,
    ) -> Result<AudioInfo, ProbeError> {
        let _ = self.entered.lock().expect("gate").send(());
        let _ = self
            .release
            .lock()
            .expect("gate")
            .recv_timeout(Duration::from_secs(30));
        self.inner.probe(file, limits, cancel)
    }
}

pub struct FixedClock(pub AtomicU64);

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        i64::try_from(self.0.load(Ordering::Relaxed)).unwrap_or(0)
    }
}

pub struct Env {
    pub dir: TempDir,
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub database: Arc<Database>,
    pub faults: Arc<Faults>,
    pub active: Arc<ActiveImports>,
    pub service: ImportService,
}

pub const NOW_MS: u64 = 1_760_000_000_000;

impl Env {
    pub fn new() -> Self {
        Self::with(ImportLimits::provisional(), Arc::new(SymphoniaProbe))
    }

    pub fn with(limits: ImportLimits, probe: Arc<dyn AudioProbe>) -> Self {
        let dir = TempDir::new();
        let root = dir.file("library");
        let db_path = dir.file("db").join("library.db");
        let database = Arc::new(Database::open(&db_path, &migrations()).expect("open database"));
        let faults = Arc::new(Faults::default());
        let active = ActiveImports::new();
        let fs = Arc::new(FaultyFs {
            inner: OsMediaFs::open(&root).expect("library dirs"),
            faults: Arc::clone(&faults),
        });
        let repo = Arc::new(FaultyRepo {
            inner: SqliteSoundRepository::new(Arc::clone(&database)),
            faults: Arc::clone(&faults),
        });
        let clock = Arc::new(FixedClock(AtomicU64::new(NOW_MS)));
        let service = ImportService::new(fs, probe, repo, clock, Arc::clone(&active), limits);
        fs::create_dir_all(dir.file("sources")).expect("sources dir");
        Self {
            dir,
            root,
            db_path,
            database,
            faults,
            active,
            service,
        }
    }

    /// Copies a committed fixture to `sources/<name>` and returns the path (the name is what the user would have picked).
    pub fn source(&self, fixture: &str, name: &str) -> PathBuf {
        let from = format!(
            "{}/tests/fixtures/audio/{fixture}",
            env!("CARGO_MANIFEST_DIR")
        );
        let to = self.dir.file("sources").join(name);
        fs::copy(from, &to).expect("copy fixture");
        to
    }

    pub fn import(&self, path: &Path) -> Result<ImportOutcome, ImportError> {
        self.service.import(path, &CancelToken::new())
    }

    pub fn sha256(&self, path: &Path) -> String {
        sha256_hex_file(path).expect("hash")
    }

    pub fn staging_files(&self) -> Vec<String> {
        let mut names: Vec<String> = list_files(&self.root.join("staging"));
        names.sort();
        names
    }

    /// Storage keys (`<shard>/<file>`) of every file in `media/`.
    pub fn media_files(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let media = self.root.join("media");
        if let Ok(shards) = fs::read_dir(&media) {
            for shard in shards.flatten() {
                for file in list_files(&shard.path()) {
                    keys.push(format!("{}/{file}", shard.file_name().to_string_lossy()));
                }
            }
        }
        keys.sort();
        keys
    }

    pub fn media_path(&self, key: &str) -> PathBuf {
        self.root.join("media").join(key)
    }

    pub fn rows(&self) -> Vec<Sound> {
        SqliteSoundRepository::new(Arc::clone(&self.database))
            .list()
            .expect("list")
    }

    /// Simulates a restart: fresh adapters over the same files and database, no faults, no running imports.
    pub fn reconcile_after_restart(&self, grace: Duration) -> ReconcileReport {
        let database =
            Arc::new(Database::open(&self.db_path, &migrations()).expect("reopen database"));
        let fs = OsMediaFs::open(&self.root).expect("library dirs");
        let repo = SqliteSoundRepository::new(database);
        reconcile(
            &fs,
            &repo,
            &ActiveImports::default(),
            SystemTime::now(),
            grace,
        )
        .expect("reconcile")
    }

    /// Reconciliation as it runs while this environment's imports may be active.
    pub fn reconcile_now(&self, grace: Duration) -> ReconcileReport {
        let fs = OsMediaFs::open(&self.root).expect("library dirs");
        let repo = SqliteSoundRepository::new(Arc::clone(&self.database));
        reconcile(&fs, &repo, &self.active, SystemTime::now(), grace).expect("reconcile")
    }

    /// The invariants ADR 0007 promises once nothing is running: every row has a file of the recorded size, nothing is left in
    /// staging, and no file in `media/` lacks a row.
    pub fn assert_consistent(&self) {
        assert_eq!(
            self.staging_files(),
            Vec::<String>::new(),
            "staging must be empty"
        );
        let rows = self.rows();
        let mut expected: Vec<String> =
            rows.iter().map(|row| row.key.as_str().to_owned()).collect();
        expected.sort();
        assert_eq!(
            self.media_files(),
            expected,
            "media files and rows must match one to one"
        );
        for row in &rows {
            let size = fs::metadata(self.media_path(row.key.as_str()))
                .expect("file exists")
                .len();
            assert_eq!(size, row.size_bytes, "recorded size matches the file");
        }
    }
}

fn list_files(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.metadata().map(|m| m.is_file()).unwrap_or(false))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

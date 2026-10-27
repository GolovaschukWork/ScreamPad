use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::application::active::ActiveImports;
use crate::application::error::StorageError;
use crate::application::import::{ImportLimits, ImportService};
use crate::application::playback::{PlaybackLimits, PlaybackService};
use crate::application::ports::FilePicker;
use crate::application::preferences::PreferencesService;
use crate::application::reconcile::{ORPHAN_GRACE, ReconcileReport, reconcile};
use crate::audio::engine::{AudioEngine, EngineConfig};
use crate::commands::error::AppError;
use crate::infrastructure::audio::cpal_backend::CpalBackend;
use crate::infrastructure::media::decode::SymphoniaClipDecoder;
use crate::infrastructure::media::fs::OsMediaFs;
use crate::infrastructure::media::probe::SymphoniaProbe;
use crate::infrastructure::sqlite::{
    Database, SqliteSettingsRepository, SqliteSoundRepository, migrations,
};
use crate::infrastructure::system::SystemClock;

/// File name of the library database inside the application data directory.
const DATABASE_FILE: &str = "library.db";
/// Directory (inside the application data directory) holding `staging/` and `media/`.
const LIBRARY_DIR: &str = "library";

/// What the last reconciliation found out about managed files, by sound id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Health {
    pub missing: HashSet<i64>,
    pub damaged: HashSet<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaStatus {
    Available,
    Missing,
    Damaged,
}

/// The managed library: database, files and the services built on them.
pub struct Library {
    pub database: Arc<Database>,
    pub repository: Arc<SqliteSoundRepository>,
    pub files: Arc<OsMediaFs>,
    pub active: Arc<ActiveImports>,
    pub importer: Arc<ImportService>,
    pub preferences: Arc<PreferencesService>,
    health: RwLock<Health>,
}

impl Library {
    pub fn new(database: Arc<Database>, root: &Path) -> std::io::Result<Self> {
        let files = Arc::new(OsMediaFs::open(root)?);
        let repository = Arc::new(SqliteSoundRepository::new(Arc::clone(&database)));
        let active = ActiveImports::new();
        let preferences = Arc::new(PreferencesService::new(
            repository.clone(),
            Arc::new(SqliteSettingsRepository::new(Arc::clone(&database))),
            Arc::new(SystemClock),
        ));
        let importer = Arc::new(ImportService::new(
            files.clone(),
            Arc::new(SymphoniaProbe),
            repository.clone(),
            Arc::new(SystemClock),
            Arc::clone(&active),
            ImportLimits::provisional(),
        ));
        Ok(Self {
            database,
            repository,
            files,
            active,
            importer,
            preferences,
            health: RwLock::new(Health::default()),
        })
    }

    /// Cleans up after interrupted imports and records which rows have a missing or damaged file. Meant to run on a
    /// background thread right after startup; it never touches a running import's files or any referenced file.
    pub fn reconcile(&self) -> Result<ReconcileReport, StorageError> {
        let report = reconcile(
            self.files.as_ref(),
            self.repository.as_ref(),
            &self.active,
            SystemTime::now(),
            ORPHAN_GRACE,
        )?;
        let mut health = self
            .health
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        health.missing = report.missing.iter().copied().collect();
        health.damaged = report.size_mismatch.iter().copied().collect();
        Ok(report)
    }

    pub fn status_of(&self, id: i64) -> MediaStatus {
        let health = self
            .health
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if health.missing.contains(&id) {
            MediaStatus::Missing
        } else if health.damaged.contains(&id) {
            MediaStatus::Damaged
        } else {
            MediaStatus::Available
        }
    }
}

/// Result of opening the library at startup. A failure is kept and reported to the UI instead of aborting the app,
/// and a damaged database is never modified or deleted automatically.
pub enum StorageState {
    Ready(Arc<Library>),
    Unavailable(AppError),
}

impl StorageState {
    pub fn open(data_dir: &Path) -> Self {
        let database = match Database::open(&data_dir.join(DATABASE_FILE), &migrations()) {
            Ok(database) => Arc::new(database),
            Err(error) => return Self::Unavailable(AppError::from(error)),
        };
        Self::ready(database, &data_dir.join(LIBRARY_DIR))
    }

    /// Builds the library services over an open database, with the media directories under `root`.
    pub fn ready(database: Arc<Database>, root: &Path) -> Self {
        match Library::new(database, root) {
            Ok(library) => Self::Ready(Arc::new(library)),
            Err(_) => Self::Unavailable(AppError::StorageIo),
        }
    }

    pub fn library(&self) -> Result<Arc<Library>, AppError> {
        match self {
            Self::Ready(library) => Ok(Arc::clone(library)),
            Self::Unavailable(error) => Err(error.clone()),
        }
    }

    pub fn database(&self) -> Result<Arc<Database>, AppError> {
        self.library().map(|library| Arc::clone(&library.database))
    }
}

/// How the user chooses a file to import. Production uses the native dialog; tests use a fixed answer.
pub struct PickerState(pub Arc<dyn FilePicker>);

/// The explicit user action that opens a locally selected package in macOS Installer.
pub struct InstallerState(
    pub  Arc<
        dyn Fn() -> Result<bool, crate::infrastructure::dialog::InstallerOpenError> + Send + Sync,
    >,
);

/// The audio side of the app: the engine that keeps the output stream alive and the service that plays library sounds.
/// Unavailable when the library could not be opened or the engine thread could not start.
pub enum PlaybackState {
    Ready(Arc<PlaybackService>),
    Unavailable(AppError),
}

impl PlaybackState {
    /// Starts the audio engine on the real default output. The engine opens the device in the background, so this returns at
    /// once even when there is no output device.
    pub fn start(storage: &StorageState) -> Self {
        let library = match storage.library() {
            Ok(library) => library,
            Err(error) => return Self::Unavailable(error),
        };
        let engine =
            match AudioEngine::start(EngineConfig::production(), || Box::new(CpalBackend::new())) {
                Ok(engine) => engine,
                Err(_) => return Self::Unavailable(AppError::Internal),
            };
        let service = PlaybackService::new(
            library.repository.clone(),
            library.files.clone(),
            Arc::new(SymphoniaClipDecoder),
            Arc::new(engine),
            PlaybackLimits::provisional(),
        );
        // Restore the saved master volume. If the settings cannot be read the output stays at unity; the UI reports the
        // damaged settings when it asks for them.
        if let Ok(settings) = library.preferences.settings() {
            let _ = service.apply_master_gain(settings.master_gain);
        }
        Self::ready(service)
    }

    pub fn ready(service: PlaybackService) -> Self {
        Self::Ready(Arc::new(service))
    }

    pub fn service(&self) -> Result<Arc<PlaybackService>, AppError> {
        match self {
            Self::Ready(service) => Ok(Arc::clone(service)),
            Self::Unavailable(error) => Err(error.clone()),
        }
    }

    /// Stops the audio stream and its thread. Called when the application exits.
    pub fn shutdown(&self) {
        if let Self::Ready(service) = self {
            service.engine().shutdown();
        }
    }
}

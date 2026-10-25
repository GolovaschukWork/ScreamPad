//! Playing a library sound (ADR 0006, ADR 0007): find the sound, make sure its managed file is the one that was imported,
//! decode it for the current output format (once, then from the cache), and hand it to the audio engine.

use std::io::{self, Seek};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::cancel::CancelToken;
use super::clip_cache::ClipCache;
use super::error::StorageError;
use super::ports::{ClipDecoder, MediaFs, ProbeError, ProbeLimits, SoundRepository};
use crate::audio::backend::OutputFormat;
use crate::audio::clip::Clip;
use crate::audio::engine::{AudioEngine, EngineError};
use crate::infrastructure::media::hash::sha256_hex_reader;

/// Limits for preparing a clip to play. The values are **provisional** until measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackLimits {
    /// Longest clip that is decoded into memory (about 23 MB per minute of stereo at 48 kHz).
    pub max_clip_ms: u64,
    pub max_decode_time: Duration,
    /// Memory budget of the decoded-clip cache.
    pub cache_bytes: usize,
}

impl PlaybackLimits {
    pub const fn provisional() -> Self {
        Self {
            max_clip_ms: 60_000,
            max_decode_time: Duration::from_secs(30),
            cache_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Why a sound could not be played. Never carries paths or file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackError {
    SoundNotFound,
    /// The managed file is gone.
    MediaMissing,
    /// The managed file is not what was imported (different size or hash) or cannot be decoded.
    MediaCorrupt,
    ClipTooLong,
    DecodeTimeout,
    /// There is no running audio output (starting, recovering, failed or stopped).
    AudioUnavailable,
    /// The engine's queues are full or it did not answer in time.
    AudioBusy,
    Storage(StorageError),
    Io(io::ErrorKind),
}

impl From<StorageError> for PlaybackError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "playback failed: {self:?}")
    }
}

impl std::error::Error for PlaybackError {}

pub struct PlaybackService {
    repository: Arc<dyn SoundRepository>,
    files: Arc<dyn MediaFs>,
    decoder: Arc<dyn ClipDecoder>,
    engine: Arc<AudioEngine>,
    cache: Mutex<ClipCache>,
    limits: PlaybackLimits,
}

impl PlaybackService {
    pub fn new(
        repository: Arc<dyn SoundRepository>,
        files: Arc<dyn MediaFs>,
        decoder: Arc<dyn ClipDecoder>,
        engine: Arc<AudioEngine>,
        limits: PlaybackLimits,
    ) -> Self {
        Self {
            repository,
            files,
            decoder,
            engine,
            cache: Mutex::new(ClipCache::new(limits.cache_bytes)),
            limits,
        }
    }

    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    /// Starts the sound `sound_id` at `gain` (1.0 is unity). May decode the file the first time; call it from a blocking task.
    pub fn play(&self, sound_id: i64, gain: f32) -> Result<(), PlaybackError> {
        // The output format can change between preparing a clip and handing it over (a device switch), so one retry is made.
        for attempt in 0..2 {
            let format = self
                .engine
                .snapshot()
                .format
                .ok_or(PlaybackError::AudioUnavailable)?;
            let clip = self.clip_for(sound_id, format)?;
            match self.engine.play(clip, gain) {
                Ok(()) => return Ok(()),
                Err(EngineError::FormatMismatch) if attempt == 0 => continue,
                Err(error) => return Err(map_engine_error(error)),
            }
        }
        Err(PlaybackError::AudioBusy)
    }

    pub fn stop_all(&self) -> Result<(), PlaybackError> {
        self.engine.stop_all().map_err(map_engine_error)
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, ClipCache> {
        // The cache only holds clips; a panic elsewhere cannot leave it half updated in a harmful way.
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clip_for(&self, sound_id: i64, format: OutputFormat) -> Result<Arc<Clip>, PlaybackError> {
        if let Some(clip) = self.cache().get(sound_id, format) {
            return Ok(clip);
        }
        let sound = self
            .repository
            .find_by_id(sound_id)?
            .ok_or(PlaybackError::SoundNotFound)?;

        // Open once and check the very bytes that will be decoded: size first (cheap), then the SHA-256 recorded at import.
        let mut file = self.files.open_media(&sound.key).map_err(map_media_error)?;
        let size = file.metadata().map_err(map_media_error)?.len();
        if size != sound.size_bytes {
            return Err(PlaybackError::MediaCorrupt);
        }
        let hash = sha256_hex_reader(&mut file).map_err(map_media_error)?;
        if hash != sound.content_hash.as_str() {
            return Err(PlaybackError::MediaCorrupt);
        }
        file.rewind().map_err(map_media_error)?;

        let limits = ProbeLimits {
            max_duration_ms: self.limits.max_clip_ms,
            max_decode_time: self.limits.max_decode_time,
        };
        let clip = self
            .decoder
            .decode(file, format, &limits, &CancelToken::new())
            .map_err(map_decode_error)?;
        self.cache().insert(sound_id, format, Arc::clone(&clip));
        Ok(clip)
    }
}

fn map_engine_error(error: EngineError) -> PlaybackError {
    match error {
        EngineError::Unavailable | EngineError::Stopped => PlaybackError::AudioUnavailable,
        EngineError::FormatMismatch | EngineError::Busy => PlaybackError::AudioBusy,
    }
}

fn map_media_error(error: io::Error) -> PlaybackError {
    match error.kind() {
        io::ErrorKind::NotFound => PlaybackError::MediaMissing,
        // A link or another non-regular file where a managed file should be.
        io::ErrorKind::InvalidInput => PlaybackError::MediaCorrupt,
        other => PlaybackError::Io(other),
    }
}

fn map_decode_error(error: ProbeError) -> PlaybackError {
    match error {
        ProbeError::Unsupported | ProbeError::Invalid => PlaybackError::MediaCorrupt,
        ProbeError::TooLong => PlaybackError::ClipTooLong,
        ProbeError::TimedOut => PlaybackError::DecodeTimeout,
        ProbeError::Cancelled => PlaybackError::AudioBusy,
        ProbeError::Io(kind) => map_media_error(io::Error::from(kind)),
    }
}

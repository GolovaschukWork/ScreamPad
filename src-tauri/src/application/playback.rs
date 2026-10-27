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
use crate::domain::playback::{MasterGain, TriggerAction};
use crate::domain::sound::Sound;
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

/// What triggering a sound did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayOutcome {
    Started,
    /// The sound's playing voices were stopped and a new one was started.
    Restarted,
    /// The sound's playing voices were stopped (toggle).
    Stopped,
    /// The sound is already playing and its mode ignores repeated triggers.
    Ignored,
}

pub struct PlaybackService {
    repository: Arc<dyn SoundRepository>,
    files: Arc<dyn MediaFs>,
    decoder: Arc<dyn ClipDecoder>,
    engine: Arc<AudioEngine>,
    cache: Mutex<ClipCache>,
    route_cache: Mutex<ClipCache>,
    limits: PlaybackLimits,
    /// Serializes the decision "is this sound playing, so what do I do" with the engine call that carries it out, so that two
    /// quick triggers of one sound cannot both decide on stale information.
    trigger: Mutex<()>,
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
            route_cache: Mutex::new(ClipCache::new(limits.cache_bytes)),
            limits,
            trigger: Mutex::new(()),
        }
    }

    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    /// Triggers the sound `sound_id` with its stored gain and play mode. May decode the file the first time; call it from a
    /// blocking task.
    ///
    /// At most 32 voices sound at once (the mixer's fixed limit). A play beyond it is refused by the audio callback, counted
    /// in the engine's `rejected_plays`, and nothing else is stopped.
    pub fn play(&self, sound_id: i64) -> Result<PlayOutcome, PlaybackError> {
        let sound = self
            .repository
            .find_by_id(sound_id)?
            .ok_or(PlaybackError::SoundNotFound)?;
        let tag = voice_tag(sound.id)?;

        // A trigger that only stops or ignores needs no clip, so it never waits for a decode.
        {
            let _decision = self.trigger_lock();
            let playing = self.engine.playing(tag).map_err(map_engine_error)?;
            match sound.play_mode.on_trigger(playing) {
                TriggerAction::Stop => {
                    self.engine.stop_tag(tag).map_err(map_engine_error)?;
                    return Ok(PlayOutcome::Stopped);
                }
                TriggerAction::Nothing => return Ok(PlayOutcome::Ignored),
                TriggerAction::Start | TriggerAction::StopThenStart => {}
            }
        }

        // The output format can change between preparing a clip and handing it over (a device switch), so one retry is made.
        for attempt in 0..2 {
            let format = self
                .engine
                .snapshot()
                .format
                .ok_or(PlaybackError::AudioUnavailable)?;
            let clip = self.clip_for(&sound, format)?;
            let route_clip = self
                .engine
                .route_snapshot()
                .format
                .and_then(|route_format| {
                    if route_format == format {
                        Some(Arc::clone(&clip))
                    } else {
                        match self.clip_for_route(&sound, route_format) {
                            Ok(clip) => Some(clip),
                            Err(_) => {
                                self.engine.record_route_clip_failure();
                                None
                            }
                        }
                    }
                });
            // The clip took time to prepare: decide again, now, together with starting it.
            let _decision = self.trigger_lock();
            let playing = self.engine.playing(tag).map_err(map_engine_error)?;
            let action = sound.play_mode.on_trigger(playing);
            match action {
                TriggerAction::Stop => {
                    self.engine.stop_tag(tag).map_err(map_engine_error)?;
                    return Ok(PlayOutcome::Stopped);
                }
                TriggerAction::Nothing => return Ok(PlayOutcome::Ignored),
                TriggerAction::StopThenStart => {
                    self.engine.stop_tag(tag).map_err(map_engine_error)?;
                }
                TriggerAction::Start => {}
            }
            match self
                .engine
                .play_tagged_routed(clip, route_clip, sound.gain.linear(), tag)
            {
                Ok(()) if action == TriggerAction::StopThenStart => {
                    return Ok(PlayOutcome::Restarted);
                }
                Ok(()) => return Ok(PlayOutcome::Started),
                Err(EngineError::FormatMismatch) if attempt == 0 => continue,
                Err(error) => return Err(map_engine_error(error)),
            }
        }
        Err(PlaybackError::AudioBusy)
    }

    /// Stops every voice of one sound. Stopping a sound that is not playing (or does not exist) is not an error.
    pub fn stop(&self, sound_id: i64) -> Result<(), PlaybackError> {
        let Ok(tag) = voice_tag(sound_id) else {
            return Ok(());
        };
        let _decision = self.trigger_lock();
        self.engine.stop_tag(tag).map_err(map_engine_error)
    }

    /// The ids of the sounds that are sounding right now, ascending.
    pub fn playing_sounds(&self) -> Result<Vec<i64>, PlaybackError> {
        let tags = self.engine.playing_tags().map_err(map_engine_error)?;
        Ok(tags
            .into_iter()
            .filter_map(|tag| i64::try_from(tag).ok())
            .collect())
    }

    /// Applies the master volume to the current and every future stream.
    pub fn apply_master_gain(&self, gain: MasterGain) -> Result<(), PlaybackError> {
        self.engine
            .set_master_gain(gain.linear())
            .map_err(map_engine_error)
    }

    fn trigger_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        // The guard protects no data, only the ordering of decisions.
        self.trigger
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn stop_all(&self) -> Result<(), PlaybackError> {
        self.engine.stop_all().map_err(map_engine_error)
    }

    fn clip_for(&self, sound: &Sound, format: OutputFormat) -> Result<Arc<Clip>, PlaybackError> {
        self.clip_for_cache(sound, format, &self.cache)
    }

    fn clip_for_route(
        &self,
        sound: &Sound,
        format: OutputFormat,
    ) -> Result<Arc<Clip>, PlaybackError> {
        self.clip_for_cache(sound, format, &self.route_cache)
    }

    fn clip_for_cache(
        &self,
        sound: &Sound,
        format: OutputFormat,
        cache: &Mutex<ClipCache>,
    ) -> Result<Arc<Clip>, PlaybackError> {
        let sound_id = sound.id;
        if let Some(clip) = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(sound_id, format)
        {
            return Ok(clip);
        }

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
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(sound_id, format, Arc::clone(&clip));
        Ok(clip)
    }
}

fn voice_tag(sound_id: i64) -> Result<u64, PlaybackError> {
    u64::try_from(sound_id).map_err(|_| PlaybackError::SoundNotFound)
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

//! Application settings and per-sound playback options (ADR 0010): validate, store, and read them back.

use std::sync::Arc;

use super::error::StorageError;
use super::ports::{Clock, SettingsRepository, SoundRepository};
use crate::domain::playback::{MasterGain, PlayMode, SoundGain};
use crate::domain::settings::{Language, Settings};
use crate::domain::sound::Sound;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferencesError {
    /// A value is outside its range or names an unsupported option.
    InvalidValue,
    SoundNotFound,
    Storage(StorageError),
}

impl From<StorageError> for PreferencesError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl std::fmt::Display for PreferencesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "preferences failed: {self:?}")
    }
}

impl std::error::Error for PreferencesError {}

pub struct PreferencesService {
    sounds: Arc<dyn SoundRepository>,
    settings: Arc<dyn SettingsRepository>,
    clock: Arc<dyn Clock>,
}

impl PreferencesService {
    pub fn new(
        sounds: Arc<dyn SoundRepository>,
        settings: Arc<dyn SettingsRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            sounds,
            settings,
            clock,
        }
    }

    /// The stored settings. A missing row is `StorageError::Corrupt`, not a default.
    pub fn settings(&self) -> Result<Settings, StorageError> {
        self.settings.get()
    }

    /// Validates and stores the language and the master volume (millibels, -6000..=0).
    pub fn update_settings(
        &self,
        language: &str,
        master_gain_millibels: i32,
    ) -> Result<Settings, PreferencesError> {
        let language = Language::parse(language).map_err(|_| PreferencesError::InvalidValue)?;
        let master_gain = MasterGain::from_millibels(master_gain_millibels)
            .map_err(|_| PreferencesError::InvalidValue)?;
        Ok(self
            .settings
            .save(language, master_gain, self.clock.now_ms())?)
    }

    /// Validates and stores a sound's gain (millibels, -6000..=600) and play mode.
    pub fn set_sound_options(
        &self,
        id: i64,
        gain_millibels: i32,
        play_mode: PlayMode,
    ) -> Result<Sound, PreferencesError> {
        let gain = SoundGain::from_millibels(gain_millibels)
            .map_err(|_| PreferencesError::InvalidValue)?;
        self.sounds
            .set_options(id, gain, play_mode, self.clock.now_ms())?
            .ok_or(PreferencesError::SoundNotFound)
    }
}

//! Playback options of a sound and the gain arithmetic shared with schema 2 (`migrations/0002_settings_and_sound_options.sql`).

use std::fmt;

/// Why a value was rejected. Carries no user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackValueError {
    GainOutOfRange,
    UnknownPlayMode,
}

impl fmt::Display for PlaybackValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GainOutOfRange => write!(f, "gain out of range"),
            Self::UnknownPlayMode => write!(f, "unknown play mode"),
        }
    }
}

impl std::error::Error for PlaybackValueError {}

/// Linear amplitude factor of a gain in millibels (1/100 dB): `10^(dB / 20)`.
fn linear_from_millibels(millibels: i32) -> f32 {
    10.0_f32.powf(millibels as f32 / 2000.0)
}

/// Gain trim of one sound: -60 dB ..= +6 dB. 0 dB is unity. -60 dB is very quiet, not mute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundGain(i32);

impl SoundGain {
    pub const MIN_MILLIBELS: i32 = -6000;
    pub const MAX_MILLIBELS: i32 = 600;
    pub const UNITY: Self = Self(0);

    pub fn from_millibels(millibels: i32) -> Result<Self, PlaybackValueError> {
        if (Self::MIN_MILLIBELS..=Self::MAX_MILLIBELS).contains(&millibels) {
            Ok(Self(millibels))
        } else {
            Err(PlaybackValueError::GainOutOfRange)
        }
    }

    pub fn millibels(self) -> i32 {
        self.0
    }

    pub fn linear(self) -> f32 {
        linear_from_millibels(self.0)
    }
}

/// Master volume: -60 dB ..= 0 dB (no boost, so the sum of voices is not made louder by the master).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MasterGain(i32);

impl MasterGain {
    pub const MIN_MILLIBELS: i32 = -6000;
    pub const MAX_MILLIBELS: i32 = 0;
    pub const UNITY: Self = Self(0);

    pub fn from_millibels(millibels: i32) -> Result<Self, PlaybackValueError> {
        if (Self::MIN_MILLIBELS..=Self::MAX_MILLIBELS).contains(&millibels) {
            Ok(Self(millibels))
        } else {
            Err(PlaybackValueError::GainOutOfRange)
        }
    }

    pub fn millibels(self) -> i32 {
        self.0
    }

    pub fn linear(self) -> f32 {
        linear_from_millibels(self.0)
    }
}

/// What triggering a sound does while that sound is already playing. Stored as text in `sounds.play_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    /// Start another voice (up to the mixer's voice limit).
    #[default]
    Overlap,
    /// Stop this sound's voices, then start a new one from the beginning.
    Restart,
    /// Do nothing while this sound is playing.
    Ignore,
    /// Stop this sound's voices if any are playing, otherwise start one.
    Toggle,
}

impl PlayMode {
    pub const ALL: [Self; 4] = [Self::Overlap, Self::Restart, Self::Ignore, Self::Toggle];

    /// The value stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overlap => "overlap",
            Self::Restart => "restart",
            Self::Ignore => "ignore",
            Self::Toggle => "toggle",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PlaybackValueError> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == value)
            .ok_or(PlaybackValueError::UnknownPlayMode)
    }

    /// What to do on a trigger, given whether the sound has voices playing.
    pub fn on_trigger(self, playing: bool) -> TriggerAction {
        match (self, playing) {
            (Self::Overlap, _) | (Self::Restart | Self::Ignore | Self::Toggle, false) => {
                TriggerAction::Start
            }
            (Self::Restart, true) => TriggerAction::StopThenStart,
            (Self::Ignore, true) => TriggerAction::Nothing,
            (Self::Toggle, true) => TriggerAction::Stop,
        }
    }
}

/// The decision a play mode makes for one trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    Start,
    StopThenStart,
    Stop,
    Nothing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_gain_accepts_exactly_the_documented_range() {
        for ok in [-6000, -1, 0, 1, 600] {
            assert_eq!(
                SoundGain::from_millibels(ok).map(SoundGain::millibels),
                Ok(ok)
            );
        }
        for bad in [i32::MIN, -6001, 601, i32::MAX] {
            assert_eq!(
                SoundGain::from_millibels(bad),
                Err(PlaybackValueError::GainOutOfRange),
                "{bad}"
            );
        }
    }

    #[test]
    fn master_gain_never_boosts() {
        assert!(MasterGain::from_millibels(0).is_ok());
        assert!(MasterGain::from_millibels(-6000).is_ok());
        assert!(MasterGain::from_millibels(1).is_err());
        assert!(MasterGain::from_millibels(-6001).is_err());
    }

    #[test]
    fn millibels_convert_to_linear_amplitude_with_db_over_20() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4 * b.max(1e-3);
        assert_eq!(SoundGain::UNITY.linear(), 1.0);
        assert!(close(
            SoundGain::from_millibels(-600).unwrap().linear(),
            0.501_187_2
        ));
        assert!(close(
            SoundGain::from_millibels(600).unwrap().linear(),
            1.995_262_3
        ));
        assert!(close(
            SoundGain::from_millibels(-6000).unwrap().linear(),
            0.001
        ));
        assert!(close(
            MasterGain::from_millibels(-2000).unwrap().linear(),
            0.1
        ));
    }

    #[test]
    fn play_modes_round_trip_through_their_stored_text() {
        for mode in PlayMode::ALL {
            assert_eq!(PlayMode::parse(mode.as_str()), Ok(mode));
        }
        for bad in ["", "loop", "OVERLAP", "overlap ", "toggle\0"] {
            assert_eq!(
                PlayMode::parse(bad),
                Err(PlaybackValueError::UnknownPlayMode),
                "{bad:?}"
            );
        }
        assert_eq!(PlayMode::default(), PlayMode::Overlap);
    }

    #[test]
    fn each_mode_decides_as_documented() {
        use TriggerAction::{Nothing, Start, Stop, StopThenStart};
        let table = [
            (PlayMode::Overlap, false, Start),
            (PlayMode::Overlap, true, Start),
            (PlayMode::Restart, false, Start),
            (PlayMode::Restart, true, StopThenStart),
            (PlayMode::Ignore, false, Start),
            (PlayMode::Ignore, true, Nothing),
            (PlayMode::Toggle, false, Start),
            (PlayMode::Toggle, true, Stop),
        ];
        for (mode, playing, expected) in table {
            assert_eq!(
                mode.on_trigger(playing),
                expected,
                "{mode:?} playing={playing}"
            );
        }
    }
}

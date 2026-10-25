//! Decodes a managed file into a [`Clip`] in the output device's format. This is the slow, allocating work that happens
//! outside the audio callback (ADR 0006): decode, resample to the device rate, then fit the device's channel count.

use std::fs::File;
use std::sync::Arc;
use std::time::Instant;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::probe::{decode_all, open};
use crate::application::cancel::CancelToken;
use crate::application::ports::{ProbeError, ProbeLimits};
use crate::audio::backend::OutputFormat;
use crate::audio::clip::Clip;
use crate::audio::convert::remap_channels;

/// Input chunk of the resampler, in frames. Any value works for whole-clip conversion; this keeps the filter state small.
const RESAMPLE_CHUNK: usize = 1024;

/// Decodes `file` into a clip. `limits.max_duration_ms` caps the clip length (and so its memory): about 23 MB per minute of
/// stereo at 48 kHz. The limit, the time budget and the cancel token apply while decoding.
pub fn decode_clip(
    file: File,
    target: OutputFormat,
    limits: &ProbeLimits,
    cancel: &CancelToken,
) -> Result<Arc<Clip>, ProbeError> {
    let started = Instant::now();
    let mut opened = open(file)?;
    let source_channels = usize::from(opened.channels);
    let source_rate = opened.sample_rate;

    let mut source: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut shape_ok = true;
    let frames = decode_all(&mut opened, limits, cancel, started, |buffer| {
        buffer.copy_to_vec_interleaved::<f32>(&mut scratch);
        shape_ok &= scratch.len().is_multiple_of(source_channels);
        source.extend_from_slice(&scratch);
    })?;
    if frames == 0 || !shape_ok || source.len() as u64 != frames * source_channels as u64 {
        return Err(ProbeError::Invalid);
    }

    // Resample at the source channel count first, so a mono file is not resampled twice.
    let resampled = if source_rate == target.sample_rate {
        source
    } else {
        resample(&source, source_channels, source_rate, target.sample_rate)?
    };
    let samples = remap_channels(resampled, source_channels, usize::from(target.channels));

    let clip =
        Clip::new(samples, target.channels, target.sample_rate).ok_or(ProbeError::Invalid)?;
    if clip.frames() == 0 {
        return Err(ProbeError::Invalid);
    }
    if clip.duration_ms() > limits.max_duration_ms.saturating_add(1) {
        return Err(ProbeError::TooLong);
    }
    Ok(clip)
}

fn resample(samples: &[f32], channels: usize, from: u32, to: u32) -> Result<Vec<f32>, ProbeError> {
    let frames = samples.len() / channels;
    let input =
        InterleavedSlice::new(samples, channels, frames).map_err(|_| ProbeError::Invalid)?;
    let mut resampler = Fft::<f32>::new(
        from as usize,
        to as usize,
        RESAMPLE_CHUNK,
        channels,
        FixedSync::Input,
    )
    .map_err(|_| ProbeError::Invalid)?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|_| ProbeError::Invalid)?;
    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn fixture(name: &str) -> File {
        File::open(format!(
            "{}/tests/fixtures/audio/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture exists")
    }

    fn limits() -> ProbeLimits {
        ProbeLimits {
            max_duration_ms: 60_000,
            max_decode_time: Duration::from_secs(30),
        }
    }

    const STEREO_48K: OutputFormat = OutputFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    fn decode(name: &str, target: OutputFormat) -> Result<Arc<Clip>, ProbeError> {
        decode_clip(fixture(name), target, &limits(), &CancelToken::new())
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Sign changes of the left channel: a 440 Hz tone crosses zero about 880 times per second.
    fn zero_crossings(clip: &Clip) -> usize {
        let left: Vec<f32> = clip
            .samples()
            .iter()
            .step_by(usize::from(clip.channels()))
            .copied()
            .collect();
        left.windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count()
    }

    #[test]
    fn a_mono_file_becomes_a_stereo_clip_at_the_device_rate() {
        let clip = decode("tone.wav", STEREO_48K).expect("decode");
        assert_eq!((clip.channels(), clip.sample_rate()), (2, 48_000));
        // 0.25 s at 48 kHz.
        assert!(
            (11_900..=12_100).contains(&clip.frames()),
            "{} frames",
            clip.frames()
        );
        assert!(clip.samples().iter().all(|s| s.is_finite()));
        let pairs_equal = clip
            .samples()
            .as_chunks::<2>()
            .0
            .iter()
            .all(|[left, right]| left == right);
        assert!(pairs_equal, "mono is copied to both channels");
    }

    #[test]
    fn resampling_keeps_the_pitch_and_the_level() {
        let native = decode(
            "tone.wav",
            OutputFormat {
                sample_rate: 8_000,
                channels: 1,
            },
        )
        .expect("native");
        let resampled = decode("tone.wav", STEREO_48K).expect("resampled");

        // 440 Hz for 0.25 s is about 220 zero crossings, before and after.
        for clip in [&native, &resampled] {
            let crossings = zero_crossings(clip);
            assert!((200..=240).contains(&crossings), "{crossings} crossings");
        }
        // The sine's amplitude is kept (within 10 percent of the RMS), so resampling did not change the loudness.
        let before = rms(native.samples());
        let after = rms(&resampled
            .samples()
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>());
        assert!(before > 0.01, "the fixture is not silent");
        assert!(
            (after / before - 1.0).abs() < 0.1,
            "rms {before} -> {after}"
        );
    }

    #[test]
    fn a_file_already_at_the_device_format_is_not_altered() {
        let clip = decode(
            "tone.wav",
            OutputFormat {
                sample_rate: 8_000,
                channels: 1,
            },
        )
        .expect("decode");
        assert_eq!(clip.frames(), 2_000, "0.25 s at 8 kHz, exactly");
        assert!(clip.samples().iter().any(|s| s.abs() > 0.05));
    }

    #[test]
    fn stereo_is_averaged_for_a_mono_device_and_every_format_decodes() {
        let mono = decode(
            "tone.ogg",
            OutputFormat {
                sample_rate: 48_000,
                channels: 1,
            },
        )
        .expect("ogg");
        assert_eq!(mono.channels(), 1);
        for name in ["tone.wav", "tone.flac", "tone.mp3", "tone.ogg"] {
            let clip = decode(name, STEREO_48K).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert!(
                (10_000..=15_000).contains(&clip.frames()),
                "{name}: {} frames",
                clip.frames()
            );
            assert!(clip.samples().iter().all(|s| s.is_finite()), "{name}");
            assert!(rms(clip.samples()) > 0.01, "{name} is not silent");
        }
    }

    /// Known limitation: symphonia does not trim the encoder delay of MP4/AAC (its format table lists "Gapless: No" for
    /// ISO/MP4), so an AAC clip starts with silence. The fixture is 0.25 s long and decodes to about 0.38 s at 48 kHz,
    /// roughly 1000 frames of the source rate. For a soundboard that is audible as a slight delay and must be trimmed
    /// before AAC files are treated as instant-trigger clips.
    #[test]
    fn aac_clips_carry_the_encoder_delay_until_it_is_trimmed() {
        let clip = decode("tone.m4a", STEREO_48K).expect("m4a");
        assert!(clip.samples().iter().all(|s| s.is_finite()));
        assert!(rms(clip.samples()) > 0.01);
        let expected = 12_000;
        assert!(
            clip.frames() > expected,
            "if this fails the delay is now trimmed: update this test and the docs"
        );
        assert!(clip.frames() < 3 * expected, "{} frames", clip.frames());
    }

    #[test]
    fn the_length_limit_bounds_the_memory_of_a_clip() {
        let strict = ProbeLimits {
            max_duration_ms: 100,
            max_decode_time: Duration::from_secs(30),
        };
        assert_eq!(
            decode_clip(
                fixture("tone.wav"),
                STEREO_48K,
                &strict,
                &CancelToken::new()
            )
            .err(),
            Some(ProbeError::TooLong)
        );
    }

    #[test]
    fn cancellation_and_the_time_budget_stop_decoding() {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            decode_clip(fixture("tone.wav"), STEREO_48K, &limits(), &cancel).err(),
            Some(ProbeError::Cancelled)
        );

        let no_time = ProbeLimits {
            max_duration_ms: 60_000,
            max_decode_time: Duration::ZERO,
        };
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(
            decode_clip(
                fixture("tone.wav"),
                STEREO_48K,
                &no_time,
                &CancelToken::new()
            )
            .err(),
            Some(ProbeError::TimedOut)
        );
    }

    #[test]
    fn damaged_and_unsupported_input_is_rejected_without_a_panic() {
        for (name, expected) in [
            (
                "garbage.bin",
                [ProbeError::Unsupported, ProbeError::Invalid],
            ),
            (
                "not_audio.wav",
                [ProbeError::Unsupported, ProbeError::Invalid],
            ),
            (
                "truncated.flac",
                [ProbeError::Invalid, ProbeError::Unsupported],
            ),
            (
                "video_with_audio.mp4",
                [ProbeError::Unsupported, ProbeError::Unsupported],
            ),
        ] {
            let error = decode(name, STEREO_48K)
                .err()
                .unwrap_or_else(|| panic!("{name} must fail"));
            assert!(expected.contains(&error), "{name}: {error:?}");
        }
    }

    #[test]
    fn an_impossible_device_format_is_an_error_not_a_panic() {
        let result = decode(
            "tone.wav",
            OutputFormat {
                sample_rate: 0,
                channels: 2,
            },
        );
        assert!(result.is_err());
        let result = decode(
            "tone.wav",
            OutputFormat {
                sample_rate: 48_000,
                channels: 0,
            },
        );
        assert!(result.is_err());
    }
}

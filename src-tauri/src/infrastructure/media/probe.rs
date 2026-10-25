//! Content-based audio inspection with `symphonia`. The file name and extension are never consulted.

use std::fs::File;
use std::io;
use std::time::Instant;

use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::well_known::{
    FORMAT_ID_FLAC, FORMAT_ID_ISOMP4, FORMAT_ID_MP3, FORMAT_ID_OGG, FORMAT_ID_WAVE,
};
use symphonia::core::formats::{FormatId, FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use crate::application::cancel::CancelToken;
use crate::application::ports::{AudioInfo, AudioProbe, ProbeError, ProbeLimits};
use crate::domain::sound::AudioFormat;

#[derive(Debug, Default, Clone, Copy)]
pub struct SymphoniaProbe;

impl AudioProbe for SymphoniaProbe {
    fn probe(
        &self,
        file: File,
        limits: &ProbeLimits,
        cancel: &CancelToken,
    ) -> Result<AudioInfo, ProbeError> {
        let started = Instant::now();
        let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
        // An empty hint: detection relies on the bytes only.
        let mut reader = symphonia::default::get_probe()
            .probe(
                &Hint::new(),
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(map_open_error)?;

        let format = classify(reader.format_info().format).ok_or(ProbeError::Unsupported)?;
        // Exactly one track: a container with more (for example a video with sound) is not a plain audio file.
        if reader.tracks().len() != 1 {
            return Err(ProbeError::Unsupported);
        }
        let track = reader
            .default_track(TrackType::Audio)
            .ok_or(ProbeError::Unsupported)?;
        let track_id = track.id;
        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err(ProbeError::Unsupported);
        };
        let sample_rate = params
            .sample_rate
            .filter(|rate| *rate > 0)
            .ok_or(ProbeError::Invalid)?;
        let channels = params
            .channels
            .as_ref()
            .map(|c| c.count())
            .filter(|c| *c > 0)
            .ok_or(ProbeError::Invalid)?;
        let channels = u16::try_from(channels).map_err(|_| ProbeError::Invalid)?;

        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|_| ProbeError::Unsupported)?;

        // Decode everything: headers can lie about the length and a truncated file only shows up while decoding. Samples
        // are discarded packet by packet, so memory stays bounded; the frame count is the truth for the duration.
        let max_frames = limits
            .max_duration_ms
            .saturating_mul(u64::from(sample_rate))
            / 1000;
        let mut frames: u64 = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(ProbeError::Cancelled);
            }
            if started.elapsed() > limits.max_decode_time {
                return Err(ProbeError::TimedOut);
            }
            let packet = match reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(error) => return Err(map_stream_error(error)),
            };
            if packet.track_id != track_id {
                continue;
            }
            let decoded = decoder.decode(&packet).map_err(map_stream_error)?;
            frames = frames.saturating_add(decoded.frames() as u64);
            if frames > max_frames {
                return Err(ProbeError::TooLong);
            }
        }
        if frames == 0 {
            return Err(ProbeError::Invalid);
        }
        let duration_ms = frames.saturating_mul(1000).div_ceil(u64::from(sample_rate));
        Ok(AudioInfo {
            format,
            duration_ms,
            sample_rate,
            channels,
        })
    }
}

fn classify(format: FormatId) -> Option<AudioFormat> {
    match format {
        FORMAT_ID_WAVE => Some(AudioFormat::Wav),
        FORMAT_ID_FLAC => Some(AudioFormat::Flac),
        FORMAT_ID_MP3 => Some(AudioFormat::Mp3),
        FORMAT_ID_OGG => Some(AudioFormat::Ogg),
        FORMAT_ID_ISOMP4 => Some(AudioFormat::M4a),
        _ => None,
    }
}

/// Errors while recognizing the container: nothing matched means "not audio we support".
fn map_open_error(error: SymphoniaError) -> ProbeError {
    match error {
        SymphoniaError::IoError(io_error) if io_error.kind() != io::ErrorKind::UnexpectedEof => {
            ProbeError::Io(io_error.kind())
        }
        SymphoniaError::IoError(_) | SymphoniaError::Unsupported(_) => ProbeError::Unsupported,
        _ => ProbeError::Invalid,
    }
}

/// Errors after the container was recognized: the file is damaged or limited.
fn map_stream_error(error: SymphoniaError) -> ProbeError {
    match error {
        SymphoniaError::IoError(io_error) if io_error.kind() != io::ErrorKind::UnexpectedEof => {
            ProbeError::Io(io_error.kind())
        }
        SymphoniaError::Unsupported(_) => ProbeError::Unsupported,
        _ => ProbeError::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn fixture(name: &str) -> File {
        let path = format!("{}/tests/fixtures/audio/{name}", env!("CARGO_MANIFEST_DIR"));
        File::open(path).expect("fixture exists")
    }

    fn limits() -> ProbeLimits {
        ProbeLimits {
            max_duration_ms: 60_000,
            max_decode_time: Duration::from_secs(30),
        }
    }

    fn probe(name: &str) -> Result<AudioInfo, ProbeError> {
        SymphoniaProbe.probe(fixture(name), &limits(), &CancelToken::new())
    }

    #[test]
    fn every_supported_format_is_recognized_from_its_content() {
        for (name, format, channels) in [
            ("tone.wav", AudioFormat::Wav, 1),
            ("tone.flac", AudioFormat::Flac, 1),
            ("tone.mp3", AudioFormat::Mp3, 1),
            ("tone.ogg", AudioFormat::Ogg, 2),
            ("tone.m4a", AudioFormat::M4a, 1),
        ] {
            let info = probe(name).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(info.format, format, "{name}");
            assert_eq!(info.sample_rate, 8000, "{name}");
            assert_eq!(info.channels, channels, "{name}");
            // 0.25 s of audio; lossy codecs add or trim a little padding.
            assert!(
                (200..=450).contains(&info.duration_ms),
                "{name}: {} ms",
                info.duration_ms
            );
        }
    }

    #[test]
    fn the_exact_duration_of_a_lossless_file_is_measured_by_decoding() {
        assert_eq!(probe("tone.wav").expect("wav").duration_ms, 250);
        assert_eq!(probe("tone.flac").expect("flac").duration_ms, 250);
    }

    #[test]
    fn the_extension_is_irrelevant_only_the_bytes_count() {
        // Copy a WAV under a misleading name: detection must not care.
        let dir = std::env::temp_dir().join(format!("screampad-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let disguised = dir.join("really-a-wav.mp3");
        std::fs::copy(
            format!(
                "{}/tests/fixtures/audio/tone.wav",
                env!("CARGO_MANIFEST_DIR")
            ),
            &disguised,
        )
        .expect("copy");
        let info = SymphoniaProbe.probe(
            File::open(&disguised).expect("open"),
            &limits(),
            &CancelToken::new(),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(info.expect("recognized").format, AudioFormat::Wav);
    }

    #[test]
    fn non_audio_and_damaged_files_are_rejected_with_typed_errors() {
        assert_eq!(probe("garbage.bin"), Err(ProbeError::Unsupported));
        assert_eq!(probe("not_audio.wav"), Err(ProbeError::Unsupported));
        assert!(matches!(
            probe("truncated_header.wav"),
            Err(ProbeError::Unsupported | ProbeError::Invalid)
        ));
        assert!(matches!(
            probe("truncated.flac"),
            Err(ProbeError::Unsupported | ProbeError::Invalid)
        ));
    }

    #[test]
    fn a_container_with_a_video_track_is_not_a_plain_audio_file() {
        assert_eq!(probe("video_with_audio.mp4"), Err(ProbeError::Unsupported));
    }

    #[test]
    fn the_duration_limit_is_enforced_by_decoding_not_by_headers() {
        let strict = ProbeLimits {
            max_duration_ms: 100,
            max_decode_time: Duration::from_secs(30),
        };
        assert_eq!(
            SymphoniaProbe.probe(fixture("tone.wav"), &strict, &CancelToken::new()),
            Err(ProbeError::TooLong)
        );
        assert_eq!(
            SymphoniaProbe.probe(fixture("tone.flac"), &strict, &CancelToken::new()),
            Err(ProbeError::TooLong)
        );
    }

    #[test]
    fn the_decode_time_budget_is_enforced() {
        let none = ProbeLimits {
            max_duration_ms: 60_000,
            max_decode_time: Duration::ZERO,
        };
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(
            SymphoniaProbe.probe(fixture("tone.wav"), &none, &CancelToken::new()),
            Err(ProbeError::TimedOut)
        );
    }

    #[test]
    fn cancellation_stops_the_probe() {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            SymphoniaProbe.probe(fixture("tone.wav"), &limits(), &cancel),
            Err(ProbeError::Cancelled)
        );
    }

    #[test]
    fn an_empty_file_is_not_audio() {
        let dir =
            std::env::temp_dir().join(format!("screampad-probe-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("empty.wav");
        std::fs::write(&path, b"").expect("write");
        let result = SymphoniaProbe.probe(
            File::open(&path).expect("open"),
            &limits(),
            &CancelToken::new(),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            matches!(result, Err(ProbeError::Unsupported | ProbeError::Invalid)),
            "{result:?}"
        );
    }
}

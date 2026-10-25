//! The encoder delay of an MP4/M4A file, read from its edit list.
//!
//! AAC encoders start the stream with priming samples (1024 or more) and pad its end. MP4 files record how much of the decoded
//! audio is real in an `elst` edit list. `symphonia` parses that atom but does not apply it, so a decoded AAC clip would begin
//! with a stretch of near-silence, which is audible as a late start. This reads just enough of the file to trim it.
//!
//! The file is untrusted. Every read is bounds-checked and returns `None` on anything unexpected, so a damaged or hostile file
//! can only mean "no trim", never a panic or a runaway allocation.

use std::io::{self, Read, Seek, SeekFrom};

/// The largest `moov` box that is read into memory.
const MAX_MOOV_BYTES: u64 = 32 * 1024 * 1024;
/// Boxes examined per level, a guard against files made of millions of tiny boxes.
const MAX_BOXES_PER_LEVEL: usize = 4096;
/// The largest plausible delay, in frames (a few seconds at any rate).
const MAX_DELAY_FRAMES: u64 = 1 << 20;

/// How much of the decoded audio is real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditTrim {
    /// The track's time unit; for audio it is normally the sample rate.
    pub timescale: u32,
    /// Leading frames to discard.
    pub delay: u64,
    /// Frames to keep after the delay, if the edit list says; otherwise play to the end.
    pub playable: Option<u64>,
}

/// Reads the audio track's single, plain edit. Returns `Ok(None)` when the data is not an MP4 or has no usable edit list.
pub fn read_edit_trim<R: Read + Seek>(reader: &mut R) -> io::Result<Option<EditTrim>> {
    let length = reader.seek(SeekFrom::End(0))?;
    let mut offset = 0_u64;
    let mut first = true;
    while offset.saturating_add(8) <= length {
        reader.seek(SeekFrom::Start(offset))?;
        let mut header = [0_u8; 8];
        reader.read_exact(&mut header)?;
        let kind = [header[4], header[5], header[6], header[7]];
        // An MP4 file starts with `ftyp`; anything else is not ours to interpret.
        if first && &kind != b"ftyp" {
            return Ok(None);
        }
        first = false;

        let declared = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let (header_len, size) = match declared {
            0 => (8, length - offset),
            1 => {
                let mut large = [0_u8; 8];
                reader.read_exact(&mut large)?;
                (16, u64::from_be_bytes(large))
            }
            other => (8, u64::from(other)),
        };
        let end = offset.checked_add(size);
        if size < header_len || end.is_none_or(|end| end > length) {
            return Ok(None);
        }
        if &kind == b"moov" {
            let payload_len = size - header_len;
            if payload_len > MAX_MOOV_BYTES {
                return Ok(None);
            }
            reader.seek(SeekFrom::Start(offset + header_len))?;
            let mut payload = vec![0_u8; usize::try_from(payload_len).unwrap_or(0)];
            reader.read_exact(&mut payload)?;
            return Ok(parse_moov(&payload));
        }
        offset += size;
    }
    Ok(None)
}

/// Child boxes of a container payload as `(kind, payload)`, stopping at the first malformed one.
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut found = Vec::new();
    let mut offset = 0_usize;
    while found.len() < MAX_BOXES_PER_LEVEL {
        let Some(header) = data.get(offset..offset + 8) else {
            break;
        };
        let declared = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = [header[4], header[5], header[6], header[7]];
        let size = if declared == 0 {
            data.len() - offset
        } else {
            declared
        };
        let Some(payload) = (size >= 8)
            .then(|| data.get(offset + 8..offset.checked_add(size)?))
            .flatten()
        else {
            break;
        };
        found.push((kind, payload));
        offset += size;
    }
    found
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .into_iter()
        .find(|(found, _)| found == kind)
        .map(|(_, payload)| payload)
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn be_u64(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn be_i16(data: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes(
        data.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

/// The timescale of an `mvhd` or `mdhd` box (the same layout in both).
fn timescale(header: &[u8]) -> Option<u32> {
    let at = match header.first()? {
        0 => 12,
        1 => 20,
        _ => return None,
    };
    be_u32(header, at).filter(|scale| *scale > 0)
}

fn parse_moov(moov: &[u8]) -> Option<EditTrim> {
    let movie_scale = timescale(child(moov, b"mvhd")?)?;
    boxes(moov)
        .into_iter()
        .filter(|(kind, _)| kind == b"trak")
        .find_map(|(_, trak)| parse_audio_trak(trak, movie_scale))
}

fn parse_audio_trak(trak: &[u8], movie_scale: u32) -> Option<EditTrim> {
    let mdia = child(trak, b"mdia")?;
    let handler = child(mdia, b"hdlr")?;
    if handler.get(8..12)? != b"soun" {
        return None;
    }
    let media_scale = timescale(child(mdia, b"mdhd")?)?;
    let elst = child(child(trak, b"edts")?, b"elst")?;

    let version = *elst.first()?;
    // Exactly one plain edit: no empty edit (leading silence), no speed change, no splicing.
    if be_u32(elst, 4)? != 1 {
        return None;
    }
    let (segment, media_time, rate_at) = match version {
        0 => (
            u64::from(be_u32(elst, 8)?),
            i64::from(be_u32(elst, 12)? as i32),
            16,
        ),
        1 => (be_u64(elst, 8)?, be_u64(elst, 16)? as i64, 24),
        _ => return None,
    };
    if be_i16(elst, rate_at)? != 1 || be_i16(elst, rate_at + 2)? != 0 {
        return None;
    }
    let delay = u64::try_from(media_time)
        .ok()
        .filter(|delay| *delay <= MAX_DELAY_FRAMES)?;
    // The edit's length is in movie time; convert it to media time, rounding to the nearest frame.
    let playable = (segment != 0)
        .then(|| {
            let scaled = (u128::from(segment) * u128::from(media_scale)
                + u128::from(movie_scale) / 2)
                / u128::from(movie_scale);
            u64::try_from(scaled).ok()
        })
        .flatten();
    if segment != 0 && playable.is_none() {
        return None;
    }
    Some(EditTrim {
        timescale: media_scale,
        delay,
        playable,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/audio/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture")
    }

    fn read(bytes: &[u8]) -> Option<EditTrim> {
        read_edit_trim(&mut Cursor::new(bytes)).unwrap_or(None)
    }

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn header(version: u8, scale: u32) -> Vec<u8> {
        // version/flags, creation, modification, timescale, duration
        let mut out = vec![version, 0, 0, 0];
        if version == 1 {
            out.extend_from_slice(&[0; 16]);
        } else {
            out.extend_from_slice(&[0; 8]);
        }
        out.extend_from_slice(&scale.to_be_bytes());
        out.extend_from_slice(&[0; 8]);
        out
    }

    fn elst_v0(entries: &[(u32, i32, i16, i16)]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 0];
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (duration, media_time, int, frac) in entries {
            out.extend_from_slice(&duration.to_be_bytes());
            out.extend_from_slice(&media_time.to_be_bytes());
            out.extend_from_slice(&int.to_be_bytes());
            out.extend_from_slice(&frac.to_be_bytes());
        }
        out
    }

    fn elst_v1(duration: u64, media_time: i64) -> Vec<u8> {
        let mut out = vec![1, 0, 0, 0, 0, 0, 0, 1];
        out.extend_from_slice(&duration.to_be_bytes());
        out.extend_from_slice(&media_time.to_be_bytes());
        out.extend_from_slice(&1_i16.to_be_bytes());
        out.extend_from_slice(&0_i16.to_be_bytes());
        out
    }

    fn file(handler: &[u8; 4], elst: &[u8]) -> Vec<u8> {
        let mut hdlr = vec![0; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0; 13]);
        let mdia = [mp4_box(b"mdhd", &header(0, 8000)), mp4_box(b"hdlr", &hdlr)].concat();
        let trak = [
            mp4_box(b"edts", &mp4_box(b"elst", elst)),
            mp4_box(b"mdia", &mdia),
        ]
        .concat();
        let moov = [mp4_box(b"mvhd", &header(0, 1000)), mp4_box(b"trak", &trak)].concat();
        [
            mp4_box(b"ftyp", b"M4A \0\0\0\0M4A "),
            mp4_box(b"moov", &moov),
        ]
        .concat()
    }

    #[test]
    fn the_edit_list_of_a_real_aac_file_gives_its_encoder_delay() {
        // 0.25 s at 8 kHz: 1024 priming frames, 2000 real frames (plus padding to a whole AAC frame).
        let trim = read(&fixture("tone.m4a")).expect("an edit list");
        assert_eq!(
            trim,
            EditTrim {
                timescale: 8000,
                delay: 1024,
                playable: Some(2000)
            }
        );
    }

    #[test]
    fn a_plain_edit_list_is_converted_from_movie_time_to_media_time() {
        let bytes = file(b"soun", &elst_v0(&[(250, 1024, 1, 0)]));
        assert_eq!(
            read(&bytes),
            Some(EditTrim {
                timescale: 8000,
                delay: 1024,
                playable: Some(2000)
            })
        );
        let version_one = file(b"soun", &elst_v1(500, 2112));
        assert_eq!(
            read(&version_one),
            Some(EditTrim {
                timescale: 8000,
                delay: 2112,
                playable: Some(4000)
            })
        );
    }

    #[test]
    fn a_zero_length_edit_means_play_to_the_end() {
        let bytes = file(b"soun", &elst_v0(&[(0, 1024, 1, 0)]));
        assert_eq!(
            read(&bytes),
            Some(EditTrim {
                timescale: 8000,
                delay: 1024,
                playable: None
            })
        );
    }

    #[test]
    fn anything_but_one_plain_edit_means_no_trim() {
        for (name, elst) in [
            ("empty edit (leading silence)", elst_v0(&[(250, -1, 1, 0)])),
            (
                "two edits",
                elst_v0(&[(100, 1024, 1, 0), (100, 4096, 1, 0)]),
            ),
            ("no edits", elst_v0(&[])),
            ("double speed", elst_v0(&[(250, 1024, 2, 0)])),
            ("fractional rate", elst_v0(&[(250, 1024, 1, 1)])),
            ("absurd delay", elst_v0(&[(250, 0x7fff_ffff, 1, 0)])),
        ] {
            assert_eq!(read(&file(b"soun", &elst)), None, "{name}");
        }
    }

    #[test]
    fn a_track_that_is_not_audio_is_ignored() {
        assert_eq!(read(&file(b"vide", &elst_v0(&[(250, 1024, 1, 0)]))), None);
    }

    #[test]
    fn data_that_is_not_an_mp4_has_no_trim() {
        for name in [
            "tone.wav",
            "tone.flac",
            "tone.mp3",
            "tone.ogg",
            "garbage.bin",
            "not_audio.wav",
        ] {
            assert_eq!(read(&fixture(name)), None, "{name}");
        }
        assert_eq!(read(&[]), None);
        assert_eq!(read(b"ftyp"), None);
    }

    #[test]
    fn lying_box_sizes_never_panic_or_over_allocate() {
        let good = fixture("tone.m4a");
        // A moov that claims to be larger than the file, larger than the memory cap, and a size of one with no room for it.
        for declared in [u32::MAX, 0x7fff_ffff, 1, 8, 9] {
            let mut bytes = good.clone();
            let moov = bytes
                .windows(4)
                .position(|window| window == b"moov")
                .expect("moov")
                - 4;
            bytes[moov..moov + 4].copy_from_slice(&declared.to_be_bytes());
            let _ = read_edit_trim(&mut Cursor::new(&bytes));
        }
    }

    #[test]
    fn every_truncation_and_every_single_byte_corruption_of_a_real_file_is_survived() {
        let good = fixture("tone.m4a");
        for length in 0..good.len() {
            let _ = read_edit_trim(&mut Cursor::new(&good[..length]));
        }
        for position in 0..good.len() {
            for pattern in [0x00_u8, 0xff, 0x80] {
                let mut bytes = good.clone();
                bytes[position] = pattern;
                let _ = read_edit_trim(&mut Cursor::new(&bytes));
            }
        }
    }
}

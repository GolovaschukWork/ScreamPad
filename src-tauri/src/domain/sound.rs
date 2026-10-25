//! Sounds in the managed library and the invariants shared with schema 1 (`migrations/0001_create_sounds.sql`).

use std::fmt;

/// Why a value was rejected. Carries no user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainError {
    InvalidContentHash,
    InvalidStorageKey,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContentHash => write!(f, "invalid content hash"),
            Self::InvalidStorageKey => write!(f, "invalid storage key"),
        }
    }
}

impl std::error::Error for DomainError {}

/// SHA-256 of the original imported bytes: exactly 64 lowercase hexadecimal characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let valid = value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(DomainError::InvalidContentHash)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// First two characters, used as the directory name that keeps directories small.
    pub fn shard(&self) -> &str {
        &self.0[..2]
    }
}

/// Container formats the library accepts. The extension of a managed file comes from here, never from the source name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    Wav,
    Flac,
    Mp3,
    Ogg,
    M4a,
}

impl AudioFormat {
    pub const ALL: [Self; 5] = [Self::Wav, Self::Flac, Self::Mp3, Self::Ogg, Self::M4a];

    /// Lowercase extension without a dot. Also the value stored in the `format` column.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
            Self::M4a => "m4a",
        }
    }

    pub fn from_extension(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.extension() == value)
    }
}

/// Safe relative storage key under the library's `media` directory: `<aa>/<hash>.<ext>`.
/// Validation mirrors the `managed_path` CHECK constraint of schema 1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    pub fn new(hash: &ContentHash, format: AudioFormat) -> Self {
        Self(format!(
            "{}/{}.{}",
            hash.shard(),
            hash.as_str(),
            format.extension()
        ))
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let charset = value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-' | b'/')
        });
        let valid = (1..=255).contains(&value.len())
            && charset
            && !value.starts_with(['/', '.'])
            && !value.ends_with('/')
            && !value.contains("//")
            && !value.contains("..")
            && !value.contains("/.");
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(DomainError::InvalidStorageKey)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Path components, for joining onto the media directory without string concatenation.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

/// A sound ready to be inserted. Timestamps are Unix milliseconds (UTC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSound {
    pub content_hash: ContentHash,
    pub display_name: String,
    pub original_file_name: Option<String>,
    pub key: StorageKey,
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub size_bytes: u64,
    pub created_at_ms: i64,
}

/// A stored sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub id: i64,
    pub content_hash: ContentHash,
    pub display_name: String,
    pub original_file_name: Option<String>,
    pub key: StorageKey,
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub size_bytes: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

const MAX_NAME_CHARS: usize = 255;

/// Reduces an untrusted file name to a bare, printable name: no directories, no control characters, bounded length.
/// Returns `None` when nothing usable is left.
pub fn sanitize_file_name(raw: &str) -> Option<String> {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    Some(trimmed.chars().take(MAX_NAME_CHARS).collect())
}

/// Default display name: the sanitized file name without its last extension, or a fallback when nothing is left.
pub fn display_name_from_file_name(sanitized: Option<&str>) -> String {
    let stem = sanitized
        .map(|name| match name.rsplit_once('.') {
            Some((stem, _)) if !stem.trim().is_empty() => stem,
            _ => name,
        })
        .map(str::trim)
        .filter(|stem| !stem.is_empty());
    stem.unwrap_or("Untitled").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn content_hash_accepts_only_64_lowercase_hex() {
        assert!(ContentHash::parse(HASH).is_ok());
        for bad in [
            "",
            &HASH[..63],
            &format!("{HASH}0"),
            &HASH.to_uppercase(),
            &format!("{}g", &HASH[..63]),
            &format!("{HASH}\0"),
        ] {
            assert_eq!(
                ContentHash::parse(bad),
                Err(DomainError::InvalidContentHash),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn storage_key_is_derived_from_the_hash_and_format() {
        let hash = ContentHash::parse(HASH).expect("hash");
        let key = StorageKey::new(&hash, AudioFormat::Flac);
        assert_eq!(key.as_str(), format!("e3/{HASH}.flac"));
        assert_eq!(
            StorageKey::parse(key.as_str()).expect("derived keys are valid"),
            key
        );
        assert_eq!(key.components().count(), 2);
    }

    #[test]
    fn storage_key_rejects_every_unsafe_shape() {
        for bad in [
            "",
            "/abs",
            "../x",
            "a/../b",
            "a//b",
            ".hidden",
            "a/.hidden",
            "a/",
            "A/upper",
            "a b",
            "a\\b",
            "c:/x",
            "é/x",
            "a/b\0.wav",
        ] {
            assert_eq!(
                StorageKey::parse(bad),
                Err(DomainError::InvalidStorageKey),
                "{bad:?}"
            );
        }
        assert_eq!(
            StorageKey::parse(&"a".repeat(256)),
            Err(DomainError::InvalidStorageKey)
        );
        assert!(StorageKey::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn every_format_round_trips_through_its_extension() {
        for format in AudioFormat::ALL {
            assert_eq!(
                AudioFormat::from_extension(format.extension()),
                Some(format)
            );
        }
        assert_eq!(AudioFormat::from_extension("exe"), None);
    }

    #[test]
    fn file_names_lose_directories_and_control_characters() {
        assert_eq!(
            sanitize_file_name("airhorn.wav").as_deref(),
            Some("airhorn.wav")
        );
        assert_eq!(
            sanitize_file_name("/Users/alice/Music/airhorn.wav").as_deref(),
            Some("airhorn.wav")
        );
        assert_eq!(
            sanitize_file_name("C:\\Users\\bob\\clip.mp3").as_deref(),
            Some("clip.mp3")
        );
        assert_eq!(
            sanitize_file_name("ev\u{0}il\n.wav").as_deref(),
            Some("evil.wav")
        );
        assert_eq!(sanitize_file_name("../..").as_deref(), None);
        assert_eq!(sanitize_file_name("   ").as_deref(), None);
        assert_eq!(sanitize_file_name("dir/").as_deref(), None);
        assert_eq!(
            sanitize_file_name(&"x".repeat(1000)).map(|n| n.chars().count()),
            Some(255)
        );
    }

    #[test]
    fn display_names_drop_the_extension_and_never_come_out_empty() {
        assert_eq!(display_name_from_file_name(Some("airhorn.wav")), "airhorn");
        assert_eq!(
            display_name_from_file_name(Some("my.great.sound.mp3")),
            "my.great.sound"
        );
        assert_eq!(display_name_from_file_name(Some(".wav")), ".wav");
        assert_eq!(display_name_from_file_name(Some("Гудок.flac")), "Гудок");
        assert_eq!(display_name_from_file_name(None), "Untitled");
    }
}

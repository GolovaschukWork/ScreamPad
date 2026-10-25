//! Application settings (schema 2, table `app_settings`).

use std::fmt;

use super::playback::MasterGain;

/// Languages the interface is translated into (ADR 0008). Adding one needs a catalog, not a migration.
pub const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "uk"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedLanguage;

impl fmt::Display for UnsupportedLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported language")
    }
}

impl std::error::Error for UnsupportedLanguage {}

/// The interface language: follow the system, or a supported language tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    System,
    Tag(&'static str),
}

impl Language {
    /// Accepts `system` and the supported tags exactly as written (lowercase).
    pub fn parse(value: &str) -> Result<Self, UnsupportedLanguage> {
        if value == "system" {
            return Ok(Self::System);
        }
        SUPPORTED_LANGUAGES
            .iter()
            .find(|tag| **tag == value)
            .map(|tag| Self::Tag(tag))
            .ok_or(UnsupportedLanguage)
    }

    /// The value stored in the database and sent to the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Tag(tag) => tag,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub language: Language,
    pub master_gain: MasterGain,
    pub updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_system_and_supported_tags_are_accepted() {
        assert_eq!(Language::parse("system"), Ok(Language::System));
        assert_eq!(Language::parse("en"), Ok(Language::Tag("en")));
        assert_eq!(Language::parse("uk"), Ok(Language::Tag("uk")));
        for bad in [
            "",
            "EN",
            "en-US",
            "de",
            "system ",
            "uk\0",
            "../x",
            "Ukrainian",
        ] {
            assert_eq!(Language::parse(bad), Err(UnsupportedLanguage), "{bad:?}");
        }
    }

    #[test]
    fn languages_round_trip_through_their_stored_text() {
        for tag in ["system", "en", "uk"] {
            assert_eq!(Language::parse(tag).map(Language::as_str), Ok(tag));
        }
    }
}

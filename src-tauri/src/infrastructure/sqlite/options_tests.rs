//! Repository tests for schema 2: per-sound playback options and application settings.

use std::sync::Arc;

use rusqlite::{Connection, params};

use super::{Database, SqliteSettingsRepository, SqliteSoundRepository, StorageError, migrations};
use crate::application::ports::{InsertOutcome, SettingsRepository, SoundRepository};
use crate::domain::playback::{MasterGain, PlayMode, SoundGain};
use crate::domain::settings::{Language, Theme};
use crate::domain::sound::{AudioFormat, ContentHash, NewSound, StorageKey};
use crate::test_support::TempDir;

const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn open(dir: &TempDir) -> Arc<Database> {
    Arc::new(Database::open(&dir.file("library.db"), &migrations()).expect("open"))
}

fn new_sound() -> NewSound {
    let hash = ContentHash::parse(HASH).expect("hash");
    NewSound {
        key: StorageKey::new(&hash, AudioFormat::Wav),
        content_hash: hash,
        display_name: "Airhorn".to_owned(),
        original_file_name: None,
        format: AudioFormat::Wav,
        duration_ms: 1500,
        sample_rate: 48_000,
        channels: 2,
        size_bytes: 1000,
        created_at_ms: 1_000,
    }
}

fn gain(millibels: i32) -> SoundGain {
    SoundGain::from_millibels(millibels).expect("gain")
}

fn master(millibels: i32) -> MasterGain {
    MasterGain::from_millibels(millibels).expect("master gain")
}

#[test]
fn an_imported_sound_starts_with_unity_gain_and_overlap() {
    let dir = TempDir::new();
    let repository = SqliteSoundRepository::new(open(&dir));
    let InsertOutcome::Inserted(sound) = repository.insert(&new_sound()).expect("insert") else {
        panic!("expected an insert");
    };
    assert_eq!(
        (sound.gain, sound.play_mode),
        (SoundGain::UNITY, PlayMode::Overlap)
    );
}

#[test]
fn options_are_stored_and_survive_reopening() {
    let dir = TempDir::new();
    let id = {
        let repository = SqliteSoundRepository::new(open(&dir));
        let InsertOutcome::Inserted(sound) = repository.insert(&new_sound()).expect("insert")
        else {
            panic!("expected an insert");
        };
        let updated = repository
            .set_options(sound.id, gain(-600), PlayMode::Toggle, 5_000)
            .expect("set")
            .expect("exists");
        assert_eq!(
            (updated.gain.millibels(), updated.play_mode),
            (-600, PlayMode::Toggle)
        );
        assert_eq!(updated.updated_at_ms, 5_000);
        // Nothing else about the sound changed.
        assert_eq!(updated.content_hash, sound.content_hash);
        assert_eq!(updated.key, sound.key);
        assert_eq!(updated.size_bytes, sound.size_bytes);
        assert_eq!(updated.created_at_ms, sound.created_at_ms);
        sound.id
    };
    let repository = SqliteSoundRepository::new(open(&dir));
    let reread = repository.find_by_id(id).expect("find").expect("exists");
    assert_eq!(
        (reread.gain.millibels(), reread.play_mode),
        (-600, PlayMode::Toggle)
    );
    assert_eq!(repository.list().expect("list")[0], reread);
}

#[test]
fn setting_options_of_an_unknown_sound_changes_nothing() {
    let dir = TempDir::new();
    let repository = SqliteSoundRepository::new(open(&dir));
    assert_eq!(
        repository
            .set_options(42, gain(100), PlayMode::Restart, 1)
            .expect("set"),
        None
    );
}

#[test]
fn a_clock_that_went_backwards_cannot_break_the_update() {
    let dir = TempDir::new();
    let repository = SqliteSoundRepository::new(open(&dir));
    let InsertOutcome::Inserted(sound) = repository.insert(&new_sound()).expect("insert") else {
        panic!("expected an insert");
    };
    let updated = repository
        .set_options(sound.id, gain(0), PlayMode::Ignore, 1)
        .expect("set")
        .expect("exists");
    assert_eq!(
        updated.updated_at_ms, sound.created_at_ms,
        "never earlier than creation"
    );
}

#[test]
fn settings_start_with_the_defaults() {
    let dir = TempDir::new();
    let settings = SqliteSettingsRepository::new(open(&dir))
        .get()
        .expect("get");
    assert_eq!(settings.language, Language::System);
    assert_eq!(settings.master_gain, MasterGain::UNITY);
    assert_eq!(settings.theme, Theme::System);
}

#[test]
fn settings_persist_across_restarts() {
    let dir = TempDir::new();
    {
        let repository = SqliteSettingsRepository::new(open(&dir));
        let saved = repository
            .save(
                Language::parse("uk").expect("language"),
                master(-1200),
                Theme::Light,
                9_000,
            )
            .expect("save");
        assert_eq!(
            (
                saved.language,
                saved.master_gain.millibels(),
                saved.updated_at_ms
            ),
            (Language::Tag("uk"), -1200, 9_000)
        );
    }
    let settings = SqliteSettingsRepository::new(open(&dir))
        .get()
        .expect("get");
    assert_eq!(
        (settings.language, settings.master_gain.millibels()),
        (Language::Tag("uk"), -1200)
    );
    assert_eq!(settings.theme, Theme::Light);
}

fn delete_settings_row(dir: &TempDir) {
    Connection::open(dir.file("library.db"))
        .expect("raw")
        .execute("DELETE FROM app_settings", [])
        .expect("delete");
}

#[test]
fn a_missing_settings_row_is_an_integrity_error_and_is_not_recreated() {
    let dir = TempDir::new();
    drop(open(&dir));
    delete_settings_row(&dir);
    let repository = SqliteSettingsRepository::new(open(&dir));
    assert_eq!(repository.get(), Err(StorageError::Corrupt));
    assert_eq!(
        repository.save(Language::System, MasterGain::UNITY, Theme::System, 1),
        Err(StorageError::Corrupt)
    );
    let rows: i64 = Connection::open(dir.file("library.db"))
        .expect("raw")
        .query_row("SELECT count(*) FROM app_settings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        rows, 0,
        "nothing may recreate the row behind the owner's back"
    );
}

#[test]
fn a_language_this_build_does_not_support_is_an_integrity_error() {
    let dir = TempDir::new();
    drop(open(&dir));
    Connection::open(dir.file("library.db"))
        .expect("raw")
        .execute("UPDATE app_settings SET language = ?1", params!["xx"])
        .expect("update");
    assert_eq!(
        SqliteSettingsRepository::new(open(&dir)).get(),
        Err(StorageError::Corrupt)
    );
}

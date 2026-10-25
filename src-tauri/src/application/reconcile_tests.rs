//! Reconciliation tests (ADR 0007): what startup cleanup may delete, what it must never touch, and what it only reports.

use std::fs;
use std::time::Duration;

use super::import::ImportOutcome;
use super::reconcile::ORPHAN_GRACE;
use super::test_env::Env;
use crate::domain::sound::{AudioFormat, ContentHash, StorageKey};
use crate::infrastructure::media::fs::backdate;

const HOURS: u64 = 3600;

fn import_one(env: &Env, fixture: &str, name: &str) -> crate::domain::sound::Sound {
    match env.import(&env.source(fixture, name)) {
        Ok(ImportOutcome::Imported(sound)) => sound,
        other => panic!("import failed: {other:?}"),
    }
}

/// Plants a file at a valid storage key that no row refers to.
fn plant_orphan(env: &Env, seed: &str, age: Duration) -> StorageKey {
    let hash = ContentHash::parse(&format!("{seed:0>64}")).expect("valid hash");
    let key = StorageKey::new(&hash, AudioFormat::Wav);
    let path = env.media_path(key.as_str());
    fs::create_dir_all(path.parent().expect("shard")).expect("shard dir");
    fs::write(&path, b"orphan bytes").expect("write orphan");
    backdate(&path, age).expect("backdate");
    key
}

#[test]
fn leftover_staging_files_are_removed_but_a_running_imports_file_is_not() {
    let env = Env::new();
    let staging = env.root.join("staging");
    fs::write(staging.join("left-by-a-crash.part"), b"partial").expect("leftover");
    fs::write(
        staging.join("owned-by-a-running-import.part"),
        b"in progress",
    )
    .expect("active");
    let _guard = env
        .active
        .register_staging("owned-by-a-running-import.part");

    let report = env.reconcile_now(ORPHAN_GRACE);
    assert_eq!(report.staging_removed, 1);
    assert_eq!(
        env.staging_files(),
        vec!["owned-by-a-running-import.part".to_owned()]
    );
}

#[test]
fn only_orphans_older_than_the_grace_period_are_removed() {
    let env = Env::new();
    let old = plant_orphan(&env, "a1", Duration::from_secs(25 * HOURS));
    let young = plant_orphan(&env, "b2", Duration::from_secs(HOURS));

    let report = env.reconcile_now(ORPHAN_GRACE);
    assert_eq!(
        (report.orphans_removed, report.orphans_within_grace),
        (1, 1)
    );
    assert!(!env.media_path(old.as_str()).exists());
    assert!(env.media_path(young.as_str()).exists());
}

#[test]
fn a_file_that_a_row_refers_to_is_never_removed_however_old_it_is() {
    let env = Env::new();
    let sound = import_one(&env, "tone.wav", "a.wav");
    let path = env.media_path(sound.key.as_str());
    backdate(&path, Duration::from_secs(400 * 24 * HOURS)).expect("backdate");

    let report = env.reconcile_now(Duration::ZERO);
    assert_eq!(report.orphans_removed, 0);
    assert!(path.exists());
    env.assert_consistent();
}

#[test]
fn an_orphan_that_a_running_import_is_publishing_is_protected() {
    let env = Env::new();
    let key = plant_orphan(&env, "c3", Duration::from_secs(48 * HOURS));
    let _guard = env.active.register_key(key.as_str());

    let report = env.reconcile_now(ORPHAN_GRACE);
    assert_eq!(report.orphans_removed, 0);
    assert!(env.media_path(key.as_str()).exists());
}

#[test]
fn files_that_are_not_valid_storage_keys_are_left_alone() {
    let env = Env::new();
    let media = env.root.join("media");
    fs::create_dir_all(media.join("zz")).expect("dir");
    let strangers = [
        media.join("zz").join("Notes.TXT"),
        media.join("zz").join(".hidden"),
        media.join("zz").join("has space.wav"),
    ];
    for stranger in &strangers {
        fs::write(stranger, b"not ours").expect("write");
        backdate(stranger, Duration::from_secs(400 * 24 * HOURS)).expect("backdate");
    }

    let report = env.reconcile_now(Duration::ZERO);
    assert_eq!(report.orphans_removed, 0);
    for stranger in &strangers {
        assert!(stranger.exists(), "{stranger:?}");
    }
}

#[test]
fn a_row_whose_file_is_gone_is_reported_not_deleted() {
    let env = Env::new();
    let kept = import_one(&env, "tone.wav", "kept.wav");
    let lost = import_one(&env, "tone.flac", "lost.flac");
    let lost_path = env.media_path(lost.key.as_str());
    let mut writable = fs::metadata(&lost_path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    writable.set_readonly(false);
    fs::set_permissions(&lost_path, writable).expect("permissions");
    fs::remove_file(&lost_path).expect("simulate antivirus or user removal");

    let report = env.reconcile_after_restart(ORPHAN_GRACE);
    assert_eq!(report.missing, vec![lost.id]);
    assert!(report.size_mismatch.is_empty());
    assert_eq!(
        env.rows().len(),
        2,
        "no row is ever deleted by reconciliation"
    );
    assert!(env.media_path(kept.key.as_str()).exists());
}

#[test]
fn a_file_with_a_different_size_than_recorded_is_reported() {
    let env = Env::new();
    let sound = import_one(&env, "tone.wav", "a.wav");
    let path = env.media_path(sound.key.as_str());
    let mut writable = fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    writable.set_readonly(false);
    fs::set_permissions(&path, writable).expect("permissions");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open")
        .set_len(10)
        .expect("truncate");

    let report = env.reconcile_after_restart(ORPHAN_GRACE);
    assert_eq!(report.size_mismatch, vec![sound.id]);
    assert!(report.missing.is_empty());
    assert!(
        path.exists(),
        "a damaged file is reported, never repaired or deleted"
    );
}

#[test]
fn a_healthy_library_produces_an_empty_report_and_a_second_run_changes_nothing() {
    let env = Env::new();
    import_one(&env, "tone.wav", "a.wav");
    import_one(&env, "tone.mp3", "b.mp3");

    let first = env.reconcile_after_restart(ORPHAN_GRACE);
    assert_eq!(first, Default::default());
    let second = env.reconcile_after_restart(ORPHAN_GRACE);
    assert_eq!(second, Default::default());
    env.assert_consistent();
}

#[test]
fn reconciliation_recovers_a_library_after_several_interrupted_imports() {
    let env = Env::new();
    import_one(&env, "tone.wav", "good.wav");

    // Crash leftovers of every kind at once.
    fs::write(env.root.join("staging").join("crash-1.part"), b"x").expect("staging");
    fs::write(env.root.join("staging").join("crash-2.part"), b"y").expect("staging");
    plant_orphan(&env, "d4", Duration::from_secs(30 * HOURS));
    plant_orphan(&env, "e5", Duration::from_secs(26 * HOURS));

    let report = env.reconcile_after_restart(ORPHAN_GRACE);
    assert_eq!((report.staging_removed, report.orphans_removed), (2, 2));
    env.assert_consistent();
}

//! Import tests: the happy path, every rejection, and a fault injected at each step of ADR 0007's table, each followed by the
//! consistency invariants (and, where a leftover is expected, by the recovery the ADR promises).

use std::fs;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::cancel::CancelToken;
use super::error::StorageError;
use super::import::{ImportError, ImportLimits, ImportOutcome};
use super::ports::{MediaFs, Published};
use super::test_env::{Env, GateProbe, PanickingProbe};
use crate::domain::sound::{AudioFormat, ContentHash, StorageKey};
use crate::infrastructure::media::fs::OsMediaFs;

fn imported(outcome: Result<ImportOutcome, ImportError>) -> crate::domain::sound::Sound {
    match outcome {
        Ok(ImportOutcome::Imported(sound)) => sound,
        other => panic!("expected a new import, got {other:?}"),
    }
}

// --- the happy path ----------------------------------------------------------------------------------------------------

#[test]
fn every_supported_format_is_imported_with_verified_metadata() {
    for (fixture, name, format) in [
        ("tone.wav", "Air horn.WAV", AudioFormat::Wav),
        ("tone.flac", "rim shot.flac", AudioFormat::Flac),
        ("tone.mp3", "Гудок.mp3", AudioFormat::Mp3),
        ("tone.ogg", "bell.ogg", AudioFormat::Ogg),
        ("tone.m4a", "clip.m4a", AudioFormat::M4a),
    ] {
        let env = Env::new();
        let source = env.source(fixture, name);
        let sound = imported(env.import(&source));

        assert_eq!(sound.format, format, "{fixture}");
        assert_eq!(
            sound.content_hash.as_str(),
            env.sha256(&source),
            "the hash is SHA-256 of the original bytes"
        );
        assert_eq!(sound.key, StorageKey::new(&sound.content_hash, format));
        assert_eq!(sound.original_file_name.as_deref(), Some(name));
        assert_eq!(
            sound.size_bytes,
            fs::metadata(&source).expect("source").len()
        );
        assert_eq!(sound.created_at_ms, super::test_env::NOW_MS as i64);
        assert_eq!(sound.sample_rate, 8000);

        let managed = env.media_path(sound.key.as_str());
        assert_eq!(
            fs::read(&managed).expect("managed"),
            fs::read(&source).expect("source"),
            "byte-identical copy"
        );
        assert!(
            fs::metadata(&managed)
                .expect("managed")
                .permissions()
                .readonly(),
            "managed files are read-only"
        );
        assert!(source.exists(), "the user's file is never touched");
        env.assert_consistent();
    }
}

#[test]
fn display_name_comes_from_the_sanitized_file_name() {
    let env = Env::new();
    let sound = imported(env.import(&env.source("tone.wav", "Air horn.v2.wav")));
    assert_eq!(sound.display_name, "Air horn.v2");
}

#[test]
fn the_format_comes_from_the_bytes_not_from_the_name() {
    let env = Env::new();
    let sound = imported(env.import(&env.source("tone.mp3", "pretending-to-be.wav")));
    assert_eq!(sound.format, AudioFormat::Mp3);
    assert!(sound.key.as_str().ends_with(".mp3"));
    env.assert_consistent();
}

// --- duplicates --------------------------------------------------------------------------------------------------------

#[test]
fn importing_the_same_bytes_again_adds_nothing() {
    let env = Env::new();
    let first = imported(env.import(&env.source("tone.wav", "one.wav")));
    let again = env.import(&env.source("tone.wav", "a different name.wav"));
    assert_eq!(again, Ok(ImportOutcome::AlreadyInLibrary(first)));
    assert_eq!(env.rows().len(), 1);
    env.assert_consistent();
}

#[test]
fn different_bytes_become_different_sounds() {
    let env = Env::new();
    imported(env.import(&env.source("tone.wav", "a.wav")));
    imported(env.import(&env.source("tone.flac", "b.flac")));
    assert_eq!(env.rows().len(), 2);
    env.assert_consistent();
}

// --- rejected sources --------------------------------------------------------------------------------------------------

#[test]
fn unusable_sources_are_rejected_with_typed_errors_and_leave_nothing_behind() {
    let env = Env::new();
    let empty = env.dir.file("sources").join("empty.wav");
    fs::write(&empty, b"").expect("empty file");
    let cases = [
        (empty, ImportError::Empty),
        (
            env.source("garbage.bin", "garbage.mp3"),
            ImportError::UnsupportedFormat,
        ),
        (
            env.source("not_audio.wav", "html-in-disguise.wav"),
            ImportError::UnsupportedFormat,
        ),
        (
            env.source("truncated.flac", "cut.flac"),
            ImportError::Corrupt,
        ),
        (
            env.source("truncated_header.wav", "cut.wav"),
            ImportError::UnsupportedFormat,
        ),
        (
            env.source("video_with_audio.mp4", "movie.mp4"),
            ImportError::UnsupportedFormat,
        ),
        (
            env.dir.file("sources").join("does-not-exist.wav"),
            ImportError::SourceUnreadable,
        ),
        (env.dir.file("sources"), ImportError::SourceNotRegularFile),
    ];
    for (path, expected) in cases {
        let result = env.import(&path);
        match (&result, &expected) {
            // A damaged file may be reported as unsupported or corrupt depending on where the decoder notices.
            (
                Err(ImportError::UnsupportedFormat | ImportError::Corrupt),
                ImportError::Corrupt | ImportError::UnsupportedFormat,
            ) => {}
            _ => assert_eq!(result, Err(expected), "{path:?}"),
        }
        env.assert_consistent();
    }
    assert!(env.rows().is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_directory_is_not_a_regular_file() {
    let env = Env::new();
    let link = env.dir.file("sources").join("link");
    std::os::unix::fs::symlink(env.dir.file("sources"), &link).expect("symlink");
    assert_eq!(env.import(&link), Err(ImportError::SourceNotRegularFile));
    env.assert_consistent();
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_real_file_is_followed_and_imports_the_target_bytes() {
    let env = Env::new();
    let target = env.source("tone.wav", "real.wav");
    let link = env.dir.file("sources").join("alias.wav");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let sound = imported(env.import(&link));
    assert_eq!(sound.content_hash.as_str(), env.sha256(&target));
    env.assert_consistent();
}

// --- resource limits ---------------------------------------------------------------------------------------------------

#[test]
fn a_file_over_the_size_limit_is_rejected_before_anything_is_copied() {
    let limits = ImportLimits {
        max_file_bytes: 1000,
        ..ImportLimits::provisional()
    };
    let env = Env::with(
        limits,
        Arc::new(crate::infrastructure::media::probe::SymphoniaProbe),
    );
    assert_eq!(
        env.import(&env.source("tone.wav", "big.wav")),
        Err(ImportError::TooLarge)
    );
    env.assert_consistent();
}

#[test]
fn audio_over_the_duration_limit_is_rejected() {
    let limits = ImportLimits {
        max_duration_ms: 100,
        ..ImportLimits::provisional()
    };
    let env = Env::with(
        limits,
        Arc::new(crate::infrastructure::media::probe::SymphoniaProbe),
    );
    assert_eq!(
        env.import(&env.source("tone.flac", "long.flac")),
        Err(ImportError::TooLong)
    );
    env.assert_consistent();
}

#[test]
fn the_decode_time_budget_stops_a_slow_file() {
    let limits = ImportLimits {
        max_decode_time: Duration::ZERO,
        ..ImportLimits::provisional()
    };
    let env = Env::with(
        limits,
        Arc::new(crate::infrastructure::media::probe::SymphoniaProbe),
    );
    assert_eq!(
        env.import(&env.source("tone.wav", "slow.wav")),
        Err(ImportError::DecodeTimeout)
    );
    env.assert_consistent();
}

#[test]
fn the_staging_quota_counts_everything_already_in_staging() {
    let limits = ImportLimits {
        max_staging_bytes: 5000,
        ..ImportLimits::provisional()
    };
    let env = Env::with(
        limits,
        Arc::new(crate::infrastructure::media::probe::SymphoniaProbe),
    );
    let foreign = env.root.join("staging").join("someone-elses.part");
    fs::write(&foreign, vec![0_u8; 3000]).expect("pre-existing staging data");

    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::StagingQuotaExceeded)
    );
    assert!(
        foreign.exists(),
        "an import never deletes staging files it does not own"
    );
    assert!(env.rows().is_empty());
}

#[test]
fn a_full_disk_while_copying_is_reported_and_cleaned_up() {
    let env = Env::new();
    *env.faults.write_after.lock().expect("faults") = Some((1000, ErrorKind::StorageFull));
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::DiskFull)
    );
    env.assert_consistent();
}

// --- cancellation and failing sources ----------------------------------------------------------------------------------

#[test]
fn an_import_cancelled_before_it_starts_does_nothing() {
    let env = Env::new();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert_eq!(
        env.service
            .import(&env.source("tone.wav", "a.wav"), &cancel),
        Err(ImportError::Cancelled)
    );
    env.assert_consistent();
}

#[test]
fn an_import_cancelled_while_copying_cleans_up() {
    let env = Env::new();
    let cancel = CancelToken::new();
    *env.faults.cancel_on_first_read.lock().expect("faults") = Some(cancel.clone());
    assert_eq!(
        env.service
            .import(&env.source("tone.wav", "a.wav"), &cancel),
        Err(ImportError::Cancelled)
    );
    env.assert_consistent();
}

#[test]
fn a_source_that_fails_mid_copy_is_reported_and_cleaned_up() {
    let env = Env::new();
    *env.faults.read_after.lock().expect("faults") = Some((1000, ErrorKind::ConnectionReset));
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::Io(ErrorKind::ConnectionReset))
    );
    env.assert_consistent();
}

#[test]
fn a_source_that_cannot_be_opened_is_reported() {
    let env = Env::new();
    *env.faults.open_source.lock().expect("faults") = Some(ErrorKind::PermissionDenied);
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::SourceUnreadable)
    );
    env.assert_consistent();
}

#[test]
fn a_source_that_changes_during_the_copy_is_rejected() {
    let env = Env::new();
    env.faults.source_changes.store(true, Ordering::Relaxed);
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::SourceChanged)
    );
    env.assert_consistent();
}

#[test]
fn a_failing_sync_of_the_staged_file_aborts_the_import() {
    let env = Env::new();
    *env.faults.sync_staging.lock().expect("faults") = Some(ErrorKind::Other);
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::Io(ErrorKind::Other))
    );
    env.assert_consistent();
}

#[test]
fn a_decoder_panic_is_contained_and_reported_as_corrupt() {
    let env = Env::with(ImportLimits::provisional(), Arc::new(PanickingProbe));
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::Corrupt)
    );
    env.assert_consistent();
}

// --- failures around the publish and the commit (ADR 0007 table) ---------------------------------------------------------

#[test]
fn a_failing_publish_leaves_no_file_and_no_row() {
    let env = Env::new();
    *env.faults.publish.lock().expect("faults") = Some(ErrorKind::PermissionDenied);
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::Io(ErrorKind::PermissionDenied))
    );
    assert!(env.media_files().is_empty());
    env.assert_consistent();
}

#[test]
fn a_full_disk_while_publishing_is_reported() {
    let env = Env::new();
    *env.faults.publish.lock().expect("faults") = Some(ErrorKind::StorageFull);
    assert_eq!(
        env.import(&env.source("tone.wav", "a.wav")),
        Err(ImportError::DiskFull)
    );
    env.assert_consistent();
}

#[test]
fn a_failed_commit_leaves_an_orphan_that_the_next_import_adopts() {
    let env = Env::new();
    let source = env.source("tone.wav", "a.wav");
    env.faults.insert.store(true, Ordering::Relaxed);

    // Between publish and commit: the file is in the library, the row is not.
    assert_eq!(
        env.import(&source),
        Err(ImportError::Storage(StorageError::Busy))
    );
    assert_eq!(
        env.media_files().len(),
        1,
        "the published file is deliberately kept (the row may or may not exist)"
    );
    assert!(env.rows().is_empty());
    assert!(env.staging_files().is_empty());

    // Repeating the import adopts the orphan instead of failing or storing a second copy.
    let sound = imported(env.import(&source));
    assert_eq!(env.media_files(), vec![sound.key.as_str().to_owned()]);
    env.assert_consistent();
}

#[test]
fn an_orphan_survives_a_restart_within_the_grace_period_and_is_removed_after_it() {
    let env = Env::new();
    env.faults.insert.store(true, Ordering::Relaxed);
    assert!(env.import(&env.source("tone.wav", "a.wav")).is_err());
    let key = env.media_files().remove(0);

    let kept = env.reconcile_after_restart(Duration::from_secs(24 * 3600));
    assert_eq!((kept.orphans_removed, kept.orphans_within_grace), (0, 1));
    assert!(env.media_path(&key).exists());

    crate::infrastructure::media::fs::backdate(
        &env.media_path(&key),
        Duration::from_secs(25 * 3600),
    )
    .expect("backdate");
    let swept = env.reconcile_after_restart(Duration::from_secs(24 * 3600));
    assert_eq!(swept.orphans_removed, 1);
    env.assert_consistent();
}

#[test]
fn a_different_file_at_the_target_key_is_never_overwritten_or_deleted() {
    let env = Env::new();
    let source = env.source("tone.wav", "a.wav");
    let hash = ContentHash::parse(&env.sha256(&source)).expect("hash");
    let key = StorageKey::new(&hash, AudioFormat::Wav);
    let squatter = env.media_path(key.as_str());
    fs::create_dir_all(squatter.parent().expect("shard")).expect("shard dir");
    fs::write(&squatter, b"someone else's bytes").expect("squatter");

    assert_eq!(env.import(&source), Err(ImportError::LibraryInconsistent));
    assert_eq!(
        fs::read(&squatter).expect("still there"),
        b"someone else's bytes"
    );
    assert!(env.staging_files().is_empty());
    assert!(env.rows().is_empty());
}

#[test]
fn publishing_over_an_identical_orphan_adopts_it() {
    let env = Env::new();
    let source = env.source("tone.wav", "a.wav");
    let hash = ContentHash::parse(&env.sha256(&source)).expect("hash");
    let key = StorageKey::new(&hash, AudioFormat::Wav);
    let target = env.media_path(key.as_str());
    fs::create_dir_all(target.parent().expect("shard")).expect("shard dir");
    fs::copy(&source, &target).expect("identical orphan");

    let sound = imported(env.import(&source));
    assert_eq!(sound.key, key);
    env.assert_consistent();
}

#[test]
fn the_file_adapter_reports_adoption_and_move_distinctly() {
    let env = Env::new();
    let fs_adapter = OsMediaFs::open(&env.root).expect("fs");
    let source = env.source("tone.wav", "a.wav");
    let hash = ContentHash::parse(&env.sha256(&source)).expect("hash");
    let key = StorageKey::new(&hash, AudioFormat::Wav);

    fs::copy(&source, env.root.join("staging").join("x.part")).expect("stage");
    assert_eq!(
        fs_adapter.publish("x.part", &key).expect("first"),
        Published::Moved
    );
    fs::copy(&source, env.root.join("staging").join("y.part")).expect("stage again");
    assert_eq!(
        fs_adapter.publish("y.part", &key).expect("second"),
        Published::AdoptedExisting
    );
    assert!(
        env.staging_files().is_empty(),
        "the redundant staged copy is discarded"
    );
}

#[test]
fn staging_names_cannot_escape_the_staging_directory() {
    let env = Env::new();
    let fs_adapter = OsMediaFs::open(&env.root).expect("fs");
    for bad in ["../evil", "a/b", "a\\b", "", ".", ".."] {
        assert!(fs_adapter.create_staging(bad).is_err(), "{bad:?}");
        assert!(fs_adapter.remove_staged(bad).is_err(), "{bad:?}");
    }
}

// --- concurrency -------------------------------------------------------------------------------------------------------

#[test]
fn a_second_import_while_one_is_running_gets_busy_and_the_first_still_completes() {
    let (probe, gate) = GateProbe::new();
    let env = Arc::new(Env::with(ImportLimits::provisional(), Arc::new(probe)));
    let first_source = env.source("tone.wav", "first.wav");
    let second_source = env.source("tone.flac", "second.flac");

    let worker = {
        let env = Arc::clone(&env);
        std::thread::spawn(move || env.import(&first_source))
    };
    gate.entered
        .recv_timeout(Duration::from_secs(30))
        .expect("the first import reached the probe");

    assert_eq!(env.import(&second_source), Err(ImportError::Busy));

    // While the first import is parked inside the probe, its staged file belongs to it: reconciliation must leave it alone.
    assert_eq!(env.staging_files().len(), 1);
    let report = env.reconcile_now(Duration::ZERO);
    assert_eq!(report.staging_removed, 0);
    assert_eq!(env.staging_files().len(), 1);

    gate.release.send(()).expect("release");
    assert!(matches!(
        worker.join().expect("worker"),
        Ok(ImportOutcome::Imported(_))
    ));
    env.assert_consistent();
}

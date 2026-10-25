//! Tests of the shipped schema: constraints, immutability, failed upgrades and compatibility with databases from earlier versions.

use std::iter::once;

use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};

use super::{Database, MIGRATIONS, StorageError, migrations};
use crate::test_support::TempDir;

const HASH_A: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const HASH_B: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// One valid row. Tests override a single field to prove that exactly that constraint rejects it.
#[derive(Clone)]
struct Row {
    hash: String,
    name: String,
    original: Option<String>,
    path: String,
    format: String,
    duration_ms: i64,
    sample_rate: i64,
    channels: i64,
    size_bytes: i64,
    created: i64,
    updated: i64,
}

impl Row {
    fn valid() -> Self {
        Self {
            hash: HASH_A.to_owned(),
            name: "Airhorn".to_owned(),
            original: Some("airhorn.wav".to_owned()),
            path: format!("e3/{HASH_A}.wav"),
            format: "wav".to_owned(),
            duration_ms: 1500,
            sample_rate: 48_000,
            channels: 2,
            size_bytes: 288_044,
            created: 1_760_000_000_000,
            updated: 1_760_000_000_000,
        }
    }

    fn insert(&self, connection: &Connection) -> Result<usize, StorageError> {
        Ok(connection.execute(
            "INSERT INTO sounds (content_hash, display_name, original_file_name, managed_path, format, duration_ms, \
             sample_rate, channels, size_bytes, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                self.hash,
                self.name,
                self.original,
                self.path,
                self.format,
                self.duration_ms,
                self.sample_rate,
                self.channels,
                self.size_bytes,
                self.created,
                self.updated
            ],
        )?)
    }
}

fn fresh() -> (TempDir, Database) {
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &migrations()).expect("open");
    (dir, db)
}

fn rejected(row: &Row, why: &str) {
    let (_dir, db) = fresh();
    let result = db.with_writer(|c| row.insert(c));
    assert_eq!(
        result,
        Err(StorageError::Constraint),
        "should be rejected: {why}"
    );
}

#[test]
fn the_first_migration_creates_the_schema_on_an_empty_database() {
    let (_dir, db) = fresh();
    assert_eq!(db.migration_outcome().from, 0);
    assert_eq!(db.migration_outcome().to, 1);
    assert!(db.migration_outcome().backup.is_none());
    db.with_writer(|c| {
        let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(version, 1);
        for (kind, name) in [
            ("table", "sounds"),
            ("index", "sounds_display_name"),
            ("trigger", "sounds_media_columns_are_immutable"),
        ] {
            let count: i64 = c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                [kind, name],
                |r| r.get(0),
            )?;
            assert_eq!(count, 1, "{kind} {name}");
        }
        let strict: i64 = c.query_row(
            "SELECT strict FROM pragma_table_list WHERE name = 'sounds'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(strict, 1, "the table must be STRICT");
        Ok(())
    })
    .expect("schema");
}

#[test]
fn running_the_migrations_again_changes_nothing() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let first = Database::open(&path, &migrations()).expect("first");
    first.with_writer(|c| Row::valid().insert(c)).expect("row");
    drop(first);
    let second = Database::open(&path, &migrations()).expect("second");
    assert_eq!(
        (
            second.migration_outcome().from,
            second.migration_outcome().to
        ),
        (1, 1)
    );
    assert!(second.migration_outcome().backup.is_none());
    let count: i64 = second
        .with_writer(|c| Ok(c.query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))?))
        .expect("count");
    assert_eq!(count, 1);
}

#[test]
fn valid_rows_are_accepted() {
    let (_dir, db) = fresh();
    let mut nested = Row::valid();
    nested.hash = HASH_B.to_owned();
    nested.path = "ab/cd/some-file_01.v2.mp3".to_owned();
    nested.original = None;
    db.with_writer(|c| {
        Row::valid().insert(c)?;
        nested.insert(c)?;
        Ok(())
    })
    .expect("both rows are valid");
}

#[test]
fn content_hash_must_be_64_lowercase_hex_characters_and_unique() {
    let valid = Row::valid();
    for (bad, why) in [
        (HASH_A.to_uppercase(), "uppercase"),
        (HASH_A[..63].to_owned(), "63 characters"),
        (format!("{HASH_A}0"), "65 characters"),
        (format!("{}g", &HASH_A[..63]), "non-hex character"),
        (String::new(), "empty"),
        (format!(" {}", &HASH_A[..63]), "whitespace"),
        (format!("{HASH_A}\0"), "NUL byte after 64 valid characters"),
        (
            format!("{}\0", &HASH_A[..63]),
            "NUL byte inside the 64 characters",
        ),
    ] {
        rejected(
            &Row {
                hash: bad,
                ..valid.clone()
            },
            why,
        );
    }
    let (_dir, db) = fresh();
    let duplicate = Row {
        path: "other/key.wav".to_owned(),
        ..valid.clone()
    };
    let result = db.with_writer(|c| {
        valid.insert(c)?;
        duplicate.insert(c)
    });
    assert_eq!(
        result,
        Err(StorageError::Constraint),
        "duplicate content_hash"
    );
}

#[test]
fn managed_path_must_be_a_unique_safe_relative_storage_key() {
    let valid = Row::valid();
    let too_long = format!("{}.wav", "a".repeat(252));
    for (bad, why) in [
        ("", "empty"),
        ("/etc/passwd", "absolute"),
        ("../escape.wav", "parent traversal"),
        ("a/../b.wav", "embedded parent traversal"),
        ("a/..", "trailing parent traversal"),
        ("a//b.wav", "empty segment"),
        (".hidden.wav", "leading dot"),
        ("a/.hidden", "hidden segment"),
        ("a/", "trailing slash"),
        ("A/upper.wav", "uppercase"),
        ("a b.wav", "space"),
        ("a\\b.wav", "backslash"),
        ("c:/x.wav", "drive letter"),
        ("a/b?.wav", "question mark"),
        ("é/x.wav", "non-ascii"),
        ("a/b\u{0}.wav", "NUL byte"),
        (too_long.as_str(), "longer than 255"),
    ] {
        rejected(
            &Row {
                path: bad.to_owned(),
                ..valid.clone()
            },
            why,
        );
    }
    let (_dir, db) = fresh();
    let same_path = Row {
        hash: HASH_B.to_owned(),
        ..valid.clone()
    };
    let result = db.with_writer(|c| {
        valid.insert(c)?;
        same_path.insert(c)
    });
    assert_eq!(
        result,
        Err(StorageError::Constraint),
        "duplicate managed_path"
    );
}

#[test]
fn other_columns_are_validated() {
    let valid = Row::valid();
    rejected(
        &Row {
            name: String::new(),
            ..valid.clone()
        },
        "empty display name",
    );
    rejected(
        &Row {
            name: "   ".to_owned(),
            ..valid.clone()
        },
        "blank display name",
    );
    rejected(
        &Row {
            original: Some(String::new()),
            ..valid.clone()
        },
        "empty original file name",
    );
    rejected(
        &Row {
            original: Some("dir/file.wav".to_owned()),
            ..valid.clone()
        },
        "original name with a path",
    );
    rejected(
        &Row {
            original: Some("dir\\file.wav".to_owned()),
            ..valid.clone()
        },
        "original name with a Windows path",
    );
    rejected(
        &Row {
            format: String::new(),
            ..valid.clone()
        },
        "empty format",
    );
    rejected(
        &Row {
            duration_ms: -1,
            ..valid.clone()
        },
        "negative duration",
    );
    rejected(
        &Row {
            sample_rate: 0,
            ..valid.clone()
        },
        "zero sample rate",
    );
    rejected(
        &Row {
            channels: 0,
            ..valid.clone()
        },
        "zero channels",
    );
    rejected(
        &Row {
            size_bytes: -1,
            ..valid.clone()
        },
        "negative size",
    );
    rejected(
        &Row {
            created: -1,
            updated: 0,
            ..valid.clone()
        },
        "negative created timestamp",
    );
    rejected(
        &Row {
            updated: valid.created - 1,
            ..valid.clone()
        },
        "updated before created",
    );
}

#[test]
fn strict_typing_rejects_the_wrong_storage_class() {
    let (_dir, db) = fresh();
    let result = db.with_writer(|c| {
        Ok(c.execute(
            "INSERT INTO sounds (content_hash, display_name, managed_path, format, duration_ms, sample_rate, channels, size_bytes, \
             created_at_ms, updated_at_ms) VALUES (?1, 'n', 'a/b.wav', 'wav', 'not a number', 48000, 2, 1, 0, 0)",
            [HASH_A],
        )?)
    });
    assert_eq!(result, Err(StorageError::Constraint));
}

#[test]
fn a_failed_insert_leaves_no_partial_row() {
    let (_dir, db) = fresh();
    let result = db.with_writer(|c| {
        let tx = c.transaction()?;
        Row::valid().insert(&tx)?;
        Row {
            hash: "bad".to_owned(),
            path: "x/y.wav".to_owned(),
            ..Row::valid()
        }
        .insert(&tx)?;
        tx.commit()?;
        Ok(())
    });
    assert_eq!(result, Err(StorageError::Constraint));
    let count: i64 = db
        .with_writer(|c| Ok(c.query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))?))
        .expect("count");
    assert_eq!(count, 0);
}

#[test]
fn media_columns_are_immutable_but_the_name_can_change() {
    let (_dir, db) = fresh();
    db.with_writer(|c| Row::valid().insert(c)).expect("row");
    for (column, value) in [
        ("content_hash", format!("'{HASH_B}'")),
        ("managed_path", "'other/key.wav'".to_owned()),
        ("format", "'mp3'".to_owned()),
        ("duration_ms", "1".to_owned()),
        ("sample_rate", "44100".to_owned()),
        ("channels", "1".to_owned()),
        ("size_bytes", "1".to_owned()),
        ("created_at_ms", "1".to_owned()),
    ] {
        let result = db
            .with_writer(|c| Ok(c.execute(&format!("UPDATE sounds SET {column} = {value}"), [])?));
        assert!(result.is_err(), "{column} must be immutable");
    }
    db.with_writer(|c| {
        Ok(c.execute(
            "UPDATE sounds SET display_name = 'Renamed', updated_at_ms = updated_at_ms + 1",
            [],
        )?)
    })
    .expect("renaming is allowed");
}

#[test]
fn a_failing_later_migration_leaves_the_previous_schema_intact() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let db = Database::open(&path, &migrations()).expect("v1");
    db.with_writer(|c| Row::valid().insert(c)).expect("row");
    drop(db);

    let extended = Migrations::new(
        MIGRATIONS
            .iter()
            .map(|m| M::up(m.sql).foreign_key_check())
            .chain(once(M::up(
                "CREATE TABLE half_applied (id INTEGER); THIS IS NOT SQL;",
            )))
            .collect(),
    );
    assert!(Database::open(&path, &extended).is_err());

    let raw = Connection::open(&path).expect("raw");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .expect("version");
    let partial: i64 = raw
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'half_applied'",
            [],
            |r| r.get(0),
        )
        .expect("partial");
    let rows: i64 = raw
        .query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))
        .expect("rows");
    assert_eq!(
        (version, partial, rows),
        (1, 0, 1),
        "version, partial schema, data"
    );
}

// --- compatibility with databases created by earlier versions -------------------------------------------------------

/// Frozen databases of every shipped schema version, in order. Add `schema_vN.sql` together with every new migration.
const FIXTURES: &[(usize, &str)] = &[(
    1,
    include_str!("../../../tests/fixtures/sqlite/schema_v1.sql"),
)];

fn schema_of(connection: &Connection) -> Vec<(String, String, String)> {
    let mut statement = connection
        .prepare("SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' AND sql IS NOT NULL ORDER BY type, name")
        .expect("prepare");
    statement
        .query_map([], |row| {
            let sql: String = row.get(2)?;
            // Comments and layout do not matter, only the statements.
            let normalized = sql
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join(" ");
            Ok((
                row.get(0)?,
                row.get(1)?,
                normalized.split_whitespace().collect::<Vec<_>>().join(" "),
            ))
        })
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

#[test]
fn there_is_a_fixture_for_every_shipped_schema_version() {
    let versions: Vec<usize> = FIXTURES.iter().map(|(version, _)| *version).collect();
    let expected: Vec<usize> = (1..=MIGRATIONS.len()).collect();
    assert_eq!(
        versions, expected,
        "add tests/fixtures/sqlite/schema_vN.sql for every migration"
    );
}

#[test]
fn databases_from_every_earlier_version_upgrade_to_the_latest_schema() {
    let fresh_schema = {
        let (_dir, db) = fresh();
        db.with_writer(|c| Ok(schema_of(c))).expect("schema")
    };
    for (version, sql) in FIXTURES {
        let dir = TempDir::new();
        let path = dir.file("library.db");
        {
            let old = Connection::open(&path).expect("create old database");
            old.execute_batch(sql).expect("load fixture");
            old.pragma_update(None, "user_version", *version as i64)
                .expect("version");
        }
        let rows_before: i64 = Connection::open(&path)
            .expect("raw")
            .query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))
            .expect("rows");
        assert!(
            rows_before > 0,
            "fixture v{version} must contain sample rows"
        );

        let db = Database::open(&path, &migrations()).expect("upgrade");
        assert_eq!(db.migration_outcome().from, *version);
        assert_eq!(db.migration_outcome().to, MIGRATIONS.len());
        db.with_writer(|c| {
            let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            assert_eq!(integrity, "ok");
            let violations: i64 =
                c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(violations, 0);
            let rows_after: i64 = c.query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))?;
            assert_eq!(
                rows_after, rows_before,
                "no rows lost upgrading from v{version}"
            );
            assert_eq!(
                schema_of(c),
                fresh_schema,
                "upgraded v{version} schema must equal a fresh database"
            );
            Ok(())
        })
        .expect("checks");
    }
}

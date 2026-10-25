use rusqlite::Connection;
use rusqlite_migration::{Error as MigrationError, M, MigrationDefinitionError, Migrations};

use super::migrations::list_backups;
use super::test_support::TempDir;
use super::{
    Database, MIGRATIONS, SHIPPED_CHECKSUMS, StorageError, checksum, migrate, migrations,
    open_reader, open_writer, quarantine_corrupt,
};

const SOUNDS: &str =
    "CREATE TABLE sounds (id INTEGER PRIMARY KEY, hash TEXT NOT NULL UNIQUE, name TEXT NOT NULL);";
const TAGS: &str = "CREATE TABLE tags (sound_id INTEGER NOT NULL REFERENCES sounds(id) ON DELETE CASCADE, tag TEXT NOT NULL);";

fn steps(count: usize) -> Migrations<'static> {
    let all = [
        SOUNDS,
        TAGS,
        "CREATE TABLE a (id INTEGER PRIMARY KEY);",
        "CREATE TABLE b (id INTEGER PRIMARY KEY);",
        "CREATE TABLE c (id INTEGER PRIMARY KEY);",
        "CREATE TABLE d (id INTEGER PRIMARY KEY);",
    ];
    Migrations::new(
        all.iter()
            .take(count)
            .map(|sql| M::up(sql).foreign_key_check())
            .collect(),
    )
}

fn user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version")
}

fn table_exists(connection: &Connection, name: &str) -> bool {
    let count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .expect("sqlite_master");
    count == 1
}

#[test]
fn production_migrations_are_valid_and_open_an_empty_database() {
    if MIGRATIONS.is_empty() {
        // The library rejects an empty list, so the production database cannot be opened until the first
        // (owner-approved) schema migration ships. This branch disappears with that migration.
        assert_eq!(
            migrations().validate(),
            Err(MigrationError::MigrationDefinition(
                MigrationDefinitionError::NoMigrationsDefined
            ))
        );
        return;
    }
    migrations().validate().expect("valid migrations");
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &migrations()).expect("open");
    assert_eq!(db.migration_outcome().from, 0);
    assert_eq!(db.migration_outcome().to, MIGRATIONS.len());
    assert!(db.migration_outcome().backup.is_none());
}

#[test]
fn shipped_migrations_never_change() {
    assert_eq!(
        MIGRATIONS.len(),
        SHIPPED_CHECKSUMS.len(),
        "record a checksum for every shipped migration"
    );
    for (migration, (name, recorded)) in MIGRATIONS.iter().zip(SHIPPED_CHECKSUMS) {
        assert_eq!(migration.name, *name);
        assert_eq!(
            checksum(migration.sql),
            *recorded,
            "shipped migration {name} was edited; add a new migration instead"
        );
    }
}

#[test]
fn checksum_detects_a_single_character_edit() {
    assert_ne!(
        checksum("CREATE TABLE a (id INTEGER);"),
        checksum("CREATE TABLE a (id INTEGEr);")
    );
}

#[test]
fn connections_apply_the_adr_settings() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let writer = open_writer(&path).expect("writer");
    let mode: String = writer
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode");
    let foreign_keys: i64 = writer
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .expect("foreign_keys");
    let synchronous: i64 = writer
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .expect("synchronous");
    let timeout: i64 = writer
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .expect("busy_timeout");
    assert_eq!(mode, "wal");
    assert_eq!(foreign_keys, 1);
    assert_eq!(synchronous, 1, "NORMAL");
    assert_eq!(timeout, 5000);

    let reader = open_reader(&path).expect("reader");
    let foreign_keys: i64 = reader
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .expect("foreign_keys");
    assert_eq!(foreign_keys, 1);
    assert!(
        reader
            .execute("CREATE TABLE nope (id INTEGER)", [])
            .is_err(),
        "readers are read-only"
    );
}

#[test]
fn bundled_sqlite_is_recent_enough() {
    let connection = Connection::open_in_memory().expect("in memory");
    let version: String = connection
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .expect("version");
    let mut parts = version
        .split('.')
        .map(|p| p.parse::<u32>().expect("numeric"));
    let (major, minor) = (parts.next().expect("major"), parts.next().expect("minor"));
    assert!(
        (major, minor) >= (3, 35),
        "VACUUM INTO and RETURNING need SQLite 3.35+, got {version}"
    );
}

#[test]
fn upgrade_is_idempotent() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    Database::open(&path, &steps(2)).expect("first open");
    let db = Database::open(&path, &steps(2)).expect("second open");
    assert_eq!(
        (db.migration_outcome().from, db.migration_outcome().to),
        (2, 2)
    );
    assert!(
        db.migration_outcome().backup.is_none(),
        "nothing pending, nothing to back up"
    );
}

#[test]
fn failed_upgrade_is_rolled_back_completely() {
    let mut connection = Connection::open_in_memory().expect("in memory");
    let broken = Migrations::new(vec![
        M::up("CREATE TABLE a (id INTEGER PRIMARY KEY);"),
        M::up("CREATE TABLE b (id INTEGER PRIMARY KEY); THIS IS NOT SQL;"),
    ]);
    let error = migrate(&mut connection, &broken, None).expect_err("must fail");
    assert!(
        matches!(error, StorageError::Sql(_) | StorageError::Migration(_)),
        "{error:?}"
    );
    assert_eq!(user_version(&connection), 0);
    assert!(
        !table_exists(&connection, "a"),
        "the earlier pending migration is rolled back too"
    );
    assert!(!table_exists(&connection, "b"));
}

#[test]
fn upgrade_of_an_existing_database_takes_a_backup_first() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let db = Database::open(&path, &steps(1)).expect("v1");
    db.with_writer(|c| Ok(c.execute("INSERT INTO sounds (hash, name) VALUES ('h', 'kept')", [])?))
        .expect("insert");
    drop(db);

    let db = Database::open(&path, &steps(2)).expect("v2");
    let backup = db.migration_outcome().backup.clone().expect("backup path");
    let snapshot = Connection::open(&backup).expect("backup opens");
    assert_eq!(
        user_version(&snapshot),
        1,
        "the backup holds the pre-upgrade schema"
    );
    let name: String = snapshot
        .query_row("SELECT name FROM sounds", [], |r| r.get(0))
        .expect("row");
    assert_eq!(name, "kept");
    assert_eq!(user_version(&Connection::open(&path).expect("live")), 2);
}

#[test]
fn fresh_database_needs_no_backup() {
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &steps(2)).expect("open");
    assert!(db.migration_outcome().backup.is_none());
    assert!(list_backups(dir.path(), "library").is_empty());
}

#[test]
fn only_the_newest_three_backups_are_kept() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    for version in 1..=6 {
        Database::open(&path, &steps(version)).expect("upgrade one step");
    }
    // Upgrades 2..=6 each took a backup; only those for the newest three target versions remain.
    let names: Vec<String> = list_backups(dir.path(), "library")
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .expect("utf-8")
                .to_owned()
        })
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for expected in ["-to-v4-", "-to-v5-", "-to-v6-"] {
        assert!(
            names.iter().any(|n| n.contains(expected)),
            "missing backup {expected}: {names:?}"
        );
    }
}

#[test]
fn a_database_newer_than_the_app_is_rejected_untouched() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    drop(Database::open(&path, &steps(2)).expect("v2"));
    Connection::open(&path)
        .expect("raw")
        .pragma_update(None, "user_version", 9)
        .expect("bump");

    let error = Database::open(&path, &steps(2))
        .err()
        .expect("must be rejected");
    assert_eq!(
        error,
        StorageError::TooNew {
            database: 9,
            supported: 2
        }
    );
    assert_eq!(
        user_version(&Connection::open(&path).expect("raw")),
        9,
        "left untouched"
    );
    assert!(list_backups(dir.path(), "library").is_empty());
}

#[test]
fn a_corrupt_file_is_reported_and_can_be_set_aside_without_losing_bytes() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let garbage = vec![0xAB_u8; 4096];
    std::fs::write(&path, &garbage).expect("write garbage");

    let error = Database::open(&path, &steps(2)).err().expect("must fail");
    assert_eq!(error, StorageError::Corrupt);

    let moved = quarantine_corrupt(&path).expect("quarantine");
    assert!(!path.exists());
    assert_eq!(std::fs::read(&moved).expect("kept"), garbage);
    Database::open(&path, &steps(2)).expect("a fresh database can be created afterwards");
}

#[test]
fn a_second_writer_gets_busy_instead_of_hanging() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let db = Database::open(&path, &steps(1)).expect("open");
    let other = open_writer(&path).expect("second writer");
    other
        .busy_timeout(std::time::Duration::from_millis(50))
        .expect("short timeout");

    db.with_writer(|writer| {
        let transaction =
            writer.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let blocked = other.execute("INSERT INTO sounds (hash, name) VALUES ('x', 'y')", []);
        assert_eq!(
            StorageError::from(blocked.expect_err("must be blocked")),
            StorageError::Busy
        );
        drop(transaction);
        Ok(())
    })
    .expect("writer");
}

#[test]
fn a_full_database_is_reported_as_full() {
    let dir = TempDir::new();
    let path = dir.file("library.db");
    let db = Database::open(&path, &steps(1)).expect("open");
    let error = db
        .with_writer(|writer| {
            let pages: i64 = writer.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            writer.execute_batch(&format!("PRAGMA max_page_count = {}", pages + 4))?;
            for n in 0..1000 {
                writer.execute(
                    "INSERT INTO sounds (hash, name) VALUES (?1, zeroblob(8000))",
                    [n.to_string()],
                )?;
            }
            Ok(())
        })
        .expect_err("the page limit must be hit");
    assert_eq!(error, StorageError::Full);
}

#[test]
fn constraint_violation_rolls_the_transaction_back() {
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &steps(2)).expect("open");
    let result = db.with_writer(|writer| {
        let transaction = writer.transaction()?;
        transaction.execute("INSERT INTO sounds (hash, name) VALUES ('h1', 'one')", [])?;
        transaction.execute(
            "INSERT INTO sounds (hash, name) VALUES ('h1', 'duplicate')",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    });
    assert_eq!(result, Err(StorageError::Constraint));
    let count: i64 = db
        .with_writer(
            |writer| Ok(writer.query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))?),
        )
        .expect("count");
    assert_eq!(
        count, 0,
        "the first insert was rolled back with the failed transaction"
    );
}

#[test]
fn foreign_keys_are_enforced_and_cascade() {
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &steps(2)).expect("open");
    db.with_writer(|writer| {
        let orphan = writer.execute("INSERT INTO tags VALUES (999, 'x')", []);
        assert_eq!(
            StorageError::from(orphan.expect_err("orphan row")),
            StorageError::Constraint
        );
        writer.execute("INSERT INTO sounds (hash, name) VALUES ('h', 'n')", [])?;
        writer.execute("INSERT INTO tags VALUES (1, 'x')", [])?;
        writer.execute("DELETE FROM sounds WHERE id = 1", [])?;
        let remaining: i64 = writer.query_row("SELECT count(*) FROM tags", [], |r| r.get(0))?;
        assert_eq!(remaining, 0);
        Ok(())
    })
    .expect("writer");
}

#[test]
fn readers_see_committed_data_while_the_writer_is_open() {
    let dir = TempDir::new();
    let db = Database::open(&dir.file("library.db"), &steps(1)).expect("open");
    db.with_writer(|w| Ok(w.execute("INSERT INTO sounds (hash, name) VALUES ('h', 'n')", [])?))
        .expect("insert");
    let reader = db.reader().expect("reader");
    let count: i64 = reader
        .query_row("SELECT count(*) FROM sounds", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 1);
}

#[test]
fn missing_directories_are_created() {
    let dir = TempDir::new();
    let nested = dir.file("a").join("b").join("library.db");
    Database::open(&nested, &steps(1)).expect("open creates parents");
    assert!(nested.exists());
}

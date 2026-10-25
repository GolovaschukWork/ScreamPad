use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use super::{Database, StorageError};
use crate::application::ports::{InsertOutcome, MediaRef, SoundRepository};
use crate::domain::sound::{AudioFormat, ContentHash, NewSound, Sound, StorageKey};

const COLUMNS: &str = "id, content_hash, display_name, original_file_name, managed_path, format, duration_ms, sample_rate, \
                       channels, size_bytes, created_at_ms, updated_at_ms";

/// `SoundRepository` over the library database. Writes use the single writer connection, reads use read-only ones.
pub struct SqliteSoundRepository {
    database: Arc<Database>,
}

impl SqliteSoundRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl SoundRepository for SqliteSoundRepository {
    fn find_by_id(&self, id: i64) -> Result<Option<Sound>, StorageError> {
        let reader = self.database.reader()?;
        let row = reader
            .query_row(
                &format!("SELECT {COLUMNS} FROM sounds WHERE id = ?1"),
                [id],
                map_row,
            )
            .optional()?;
        row.map(|parsed| parsed.map_err(|_| StorageError::Corrupt))
            .transpose()
    }

    fn find_by_hash(&self, hash: &ContentHash) -> Result<Option<Sound>, StorageError> {
        let reader = self.database.reader()?;
        find_by_hash(&reader, hash)
    }

    fn insert(&self, sound: &NewSound) -> Result<InsertOutcome, StorageError> {
        self.database.with_writer(|writer| {
            // IMMEDIATE takes the write lock up front, so the duplicate check and the insert cannot interleave with another writer.
            let transaction = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let inserted: Option<i64> = transaction
                .query_row(
                    "INSERT INTO sounds (content_hash, display_name, original_file_name, managed_path, format, duration_ms, \
                     sample_rate, channels, size_bytes, created_at_ms, updated_at_ms) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10) \
                     ON CONFLICT(content_hash) DO NOTHING RETURNING id",
                    params![
                        sound.content_hash.as_str(),
                        sound.display_name,
                        sound.original_file_name,
                        sound.key.as_str(),
                        sound.format.extension(),
                        to_i64(sound.duration_ms)?,
                        i64::from(sound.sample_rate),
                        i64::from(sound.channels),
                        to_i64(sound.size_bytes)?,
                        sound.created_at_ms,
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            let stored = find_by_hash(&transaction, &sound.content_hash)?.ok_or(StorageError::Corrupt)?;
            transaction.commit()?;
            Ok(if inserted.is_some() { InsertOutcome::Inserted(stored) } else { InsertOutcome::Duplicate(stored) })
        })
    }

    fn key_exists(&self, key: &StorageKey) -> Result<bool, StorageError> {
        let reader = self.database.reader()?;
        let found: Option<i64> = reader
            .query_row(
                "SELECT 1 FROM sounds WHERE managed_path = ?1",
                [key.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    fn media_refs(&self) -> Result<Vec<MediaRef>, StorageError> {
        let reader = self.database.reader()?;
        let mut statement =
            reader.prepare("SELECT id, managed_path, size_bytes FROM sounds ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut refs = Vec::new();
        for row in rows {
            let (id, key, size) = row?;
            refs.push(MediaRef {
                id,
                key: StorageKey::parse(&key).map_err(|_| StorageError::Corrupt)?,
                size_bytes: u64::try_from(size).map_err(|_| StorageError::Corrupt)?,
            });
        }
        Ok(refs)
    }

    fn list(&self) -> Result<Vec<Sound>, StorageError> {
        let reader = self.database.reader()?;
        let mut statement = reader.prepare(&format!("SELECT {COLUMNS} FROM sounds ORDER BY id"))?;
        let rows = statement.query_map([], map_row)?;
        let mut sounds = Vec::new();
        for row in rows {
            sounds.push(row?.map_err(|_| StorageError::Corrupt)?);
        }
        Ok(sounds)
    }
}

fn find_by_hash(
    connection: &Connection,
    hash: &ContentHash,
) -> Result<Option<Sound>, StorageError> {
    let row = connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM sounds WHERE content_hash = ?1"),
            [hash.as_str()],
            map_row,
        )
        .optional()?;
    row.map(|parsed| parsed.map_err(|_| StorageError::Corrupt))
        .transpose()
}

/// Outer error: SQLite. Inner error: the row violates a domain invariant, which the schema's CHECKs make impossible
/// unless the file was edited by hand.
fn map_row(row: &Row<'_>) -> rusqlite::Result<Result<Sound, ()>> {
    let hash: String = row.get(1)?;
    let key: String = row.get(4)?;
    let format: String = row.get(5)?;
    let parsed = (|| {
        Some(Sound {
            id: row.get(0).ok()?,
            content_hash: ContentHash::parse(&hash).ok()?,
            display_name: row.get(2).ok()?,
            original_file_name: row.get(3).ok()?,
            key: StorageKey::parse(&key).ok()?,
            format: AudioFormat::from_extension(&format)?,
            duration_ms: u64::try_from(row.get::<_, i64>(6).ok()?).ok()?,
            sample_rate: u32::try_from(row.get::<_, i64>(7).ok()?).ok()?,
            channels: u16::try_from(row.get::<_, i64>(8).ok()?).ok()?,
            size_bytes: u64::try_from(row.get::<_, i64>(9).ok()?).ok()?,
            created_at_ms: row.get(10).ok()?,
            updated_at_ms: row.get(11).ok()?,
        })
    })();
    Ok(parsed.ok_or(()))
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::Constraint)
}

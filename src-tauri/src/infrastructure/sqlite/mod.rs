//! SQLite access (ADR 0005): one writer connection, read-only connections, atomic forward-only migrations.

mod connection;
mod database;
mod error;
mod migrations;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use connection::{open_reader, open_writer, quarantine_corrupt};
pub use database::Database;
pub use error::StorageError;
pub use migrations::{
    BackupTarget, MIGRATIONS, MigrationOutcome, SHIPPED_CHECKSUMS, ShippedMigration, checksum,
    migrate, migrations,
};

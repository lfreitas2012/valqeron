//! Schema migrations for the SQLite driver.
//!
//! This module contains a list of SQL scripts that define the database schema and a function to
//! apply them to a database connection. The scripts are ordered by schema version so that the
//! database can be upgraded incrementally, e.g., from version 1 to 2, 2 to 3, etc.
//!
//! The scripts are embedded in the binary, and the binary's version number is used to determine
//! which migrations to apply. This means that if the database schema changes, the binary must be
//! upgraded, and the database must be re-migrated.

use rusqlite::{Connection, TransactionBehavior};

use crate::sqlite::error::SqliteError;

/// Ordered list of embedded migration scripts.
pub const MIGRATIONS: &[&str] = &[include_str!(
    "../../../../migrations/001_create_initial_issuer_schema.sql"
)];

/// Apply any pending migrations to `connection`, advancing `user_version` as it goes.
/// Idempotent: already-applied migrations are skipped.
///
/// # Errors
///
/// Returns [`SqliteError::UnknownSchemaVersion`] if the on-disk schema is newer than the
/// migrations this binary knows about, or [`SqliteError::Migration`] if a script fails to apply.
pub fn run(connection: &mut Connection) -> Result<(), SqliteError> {
    fn migration_err(source: impl std::error::Error + Send + Sync + 'static) -> SqliteError {
        SqliteError::Migration {
            source: Box::new(source),
        }
    }

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(migration_err)?;

    if current_version as usize > MIGRATIONS.len() {
        return Err(SqliteError::UnknownSchemaVersion {
            found: current_version,
            known: MIGRATIONS.len(),
        });
    }

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let migration_version = (index + 1) as i64;
        if migration_version <= current_version {
            continue;
        }

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(migration_err)?;

        tx.execute_batch(sql).map_err(migration_err)?;

        tx.pragma_update(None, "user_version", migration_version)
            .map_err(migration_err)?;

        tx.commit().map_err(migration_err)?;
    }

    Ok(())
}

//! Schema migrations for the SQLite backend.
//!
//! Migrations are embedded at compile time and applied in order. The database's
//! `user_version` pragma tracks how many migrations have been applied, so
//! opening an already-migrated database is a harmless no-op and a database whose
//! schema is newer than this binary understands is rejected rather than silently
//! mishandled.

use rusqlite::{Connection, TransactionBehavior};

use crate::sqlite::error::SqliteDbError;

/// Ordered list of embedded migration scripts. The index (1-based) is the
/// schema version a script advances the database to.
pub const MIGRATIONS: &[&str] = &[include_str!(
    "../../../../migrations/001_create_initial_issuer_schema.sql"
)];

/// Apply any pending migrations to `connection`, advancing `user_version` as it
/// goes. Idempotent: already-applied migrations are skipped.
///
/// # Errors
///
/// Returns [`SqliteDbError::UnknownSchemaVersion`] if the on-disk schema
/// is newer than the migrations this binary knows about, or
/// [`SqliteDbError::Migration`] if a script fails to apply.
pub fn run(connection: &mut Connection) -> Result<(), SqliteDbError> {
    fn migration_err(source: impl std::error::Error + Send + Sync + 'static) -> SqliteDbError {
        SqliteDbError::Migration {
            source: Box::new(source),
        }
    }

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(migration_err)?;

    if current_version as usize > MIGRATIONS.len() {
        return Err(SqliteDbError::UnknownSchemaVersion {
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

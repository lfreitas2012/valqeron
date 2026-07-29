//! SQLite implementation of the core [`StorageBackend`] abstraction.
//!
//! [`SqliteBackend`] adapts the low-level [`Database`](crate::sqlite::Database)
//! driver (writer connection + reader pool + migrations) onto the backend-neutral
//! contract defined in `valqeron-core`, so applications talk only to
//! [`Store`](valqeron_core::Store) and the domain repository traits.

use std::path::Path;

use valqeron_core::{IssuerRepository, StorageBackend, StorageConfig, StorageError, Store};

use crate::sqlite::{Database, DatabaseConfig, SqliteDataDriverError, SqliteIssuerRepository};

/// Map the SQLite driver's error type onto the driver-neutral [`StorageError`].
fn map_driver_error(err: SqliteDataDriverError) -> StorageError {
    match err {
        SqliteDataDriverError::Connection { source } | SqliteDataDriverError::Pragma { source } => {
            StorageError::Open {
                source: Box::new(source),
            }
        }
        SqliteDataDriverError::DryRun { source } => StorageError::DryRun {
            source: Box::new(source),
        },
        SqliteDataDriverError::Migration { source } => StorageError::Migration { source },
        SqliteDataDriverError::UnknownSchemaVersion { found, known } => {
            StorageError::SchemaTooNew { found, known }
        }
        SqliteDataDriverError::InvalidPoolSize => {
            StorageError::Config("reader pool size must be at least 1".into())
        }
    }
}

impl From<StorageConfig> for DatabaseConfig {
    fn from(cfg: StorageConfig) -> Self {
        DatabaseConfig {
            reader_pool_size: cfg.reader_pool_size,
        }
    }
}

/// A [`StorageBackend`] backed by SQLite.
pub struct SqliteBackend {
    db: Database,
}

impl SqliteBackend {
    /// Open (or create) a SQLite database at `path`, applying pending migrations.
    pub fn open(path: impl AsRef<Path>, config: StorageConfig) -> Result<Self, StorageError> {
        let db = Database::open_with_config(path, config.into()).map_err(map_driver_error)?;
        Ok(Self { db })
    }

    /// Open an isolated in-memory SQLite database (useful for tests).
    pub fn open_in_memory(config: StorageConfig) -> Result<Self, StorageError> {
        let db = Database::open_in_memory_with_config(config.into()).map_err(map_driver_error)?;
        Ok(Self { db })
    }
}

impl StorageBackend for SqliteBackend {
    fn issuers(&self) -> Box<dyn IssuerRepository> {
        Box::new(SqliteIssuerRepository::new(self.db.handle()))
    }

    fn migrate(&self) -> Result<(), StorageError> {
        // Migrations are applied on open; opening again is a harmless no-op that
        // also validates the on-disk schema is not newer than we understand.
        Ok(())
    }

    fn dry_run(&self, f: &mut dyn FnMut(&dyn IssuerRepository)) -> Result<(), StorageError> {
        self.db
            .dry_run(|handle| {
                let repo = SqliteIssuerRepository::new(handle.clone());
                f(&repo);
            })
            .map_err(map_driver_error)
    }
}

/// Convenience constructor: open a SQLite-backed [`Store`] at `path`.
pub fn open_sqlite(path: impl AsRef<Path>, config: StorageConfig) -> Result<Store, StorageError> {
    Ok(Store::new(Box::new(SqliteBackend::open(path, config)?)))
}

/// Convenience constructor: open an in-memory SQLite-backed [`Store`].
pub fn open_sqlite_in_memory(config: StorageConfig) -> Result<Store, StorageError> {
    Ok(Store::new(Box::new(SqliteBackend::open_in_memory(config)?)))
}

//! The SQLite-backed [`StorageEngine`] adapter.

use std::path::Path;

use valqeron_core::{Repositories, StorageEngine, StorageError};

use crate::sqlite::connection::{Database, DatabaseConfig};
use crate::sqlite::issuer::SqliteIssuerRepository;

/// The SQLite-backed [`StorageEngine`].
///
/// This is the concrete adapter the application wires into a
/// [`PersistenceManager`](valqeron_core::PersistenceManager). All SQLite specifics (connection
/// pooling, pragmas, SQL, migrations) live behind this type in private submodules; the only public
/// surface is this engine plus its [`DatabaseConfig`](crate::DatabaseConfig) and
/// [`SqliteError`](crate::SqliteError).
pub struct SqliteStorageEngine {
    db: Database,
}

impl SqliteStorageEngine {
    /// Open (or create) a SQLite-backed store at `path`, applying any pending migrations.
    ///
    /// Driver errors are translated into the domain's [`StorageError`] so callers never depend on
    /// the SQLite-specific error type.
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self, StorageError> {
        let db = Database::open_with_config(path, config)?;
        Ok(Self { db })
    }

    /// Open an isolated in-memory store (primarily for tests), applying migrations.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let db = Database::open_in_memory()?;
        Ok(Self { db })
    }
}

impl StorageEngine for SqliteStorageEngine {
    type Issuers = SqliteIssuerRepository;

    fn repositories(&self) -> Repositories<Self> {
        Repositories {
            issuers: SqliteIssuerRepository::new(self.db.handle()),
        }
    }

    fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Repositories<Self>) -> T,
    {
        self.db.dry_run(|handle| {
            let repositories = Repositories {
                issuers: SqliteIssuerRepository::new(handle.clone()),
            };
            f(&repositories)
        })
    }
}

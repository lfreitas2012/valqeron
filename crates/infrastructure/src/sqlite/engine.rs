use std::path::Path;

use valqeron_core::{Repositories, StorageEngine, StorageError};

use crate::sqlite::connection::{Database, DatabaseConfig};
use crate::sqlite::issuer::SqliteIssuerRepository;

pub struct SqliteStorageEngine {
    db: Database,
}

impl SqliteStorageEngine {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self, StorageError> {
        let db = Database::open_with_config(path, config)?;
        Ok(Self { db })
    }

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

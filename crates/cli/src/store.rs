use valqeron_core::{IssuerRepository, PersistenceManager, Repositories, StorageEngine};
use valqeron_infrastructure::{DatabaseConfig, SqliteStorageEngine};

use crate::config::ValqeronConfig;
use crate::error::AppResult;

pub fn open(config: &ValqeronConfig) -> AppResult<PersistenceManager<SqliteStorageEngine>> {
    let db_config = DatabaseConfig {
        reader_pool_size: config.reader_pool_size(),
        synchronous: config.synchronous(),
        ..DatabaseConfig::default()
    };
    let engine = SqliteStorageEngine::open(config.db_path(), db_config)?;
    Ok(PersistenceManager::new(engine))
}

pub struct Repos<'a> {
    issuers: &'a dyn IssuerRepository,
}

impl Repos<'_> {
    pub fn issuers(&self) -> &dyn IssuerRepository {
        self.issuers
    }
}

pub fn repos<E: StorageEngine>(repositories: &Repositories<E>) -> Repos<'_> {
    Repos {
        issuers: &repositories.issuers,
    }
}

use crate::sqlite::db::Database;
use crate::sqlite::driver::Synchronous;
use crate::sqlite::repository::SqliteIssuerRepository;
use std::thread;
use std::time::Duration;
use valqeron_core::{Repositories, StorageEngine};

mod db;
mod driver;
mod error;
mod mapping;
mod migrations;
mod models;
mod queries;
mod repository;

/// Configuration for opening a [`DatabaseConnection`].
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Number of read-only connections held in the reader pool.
    pub reader_pool_size: usize,
    /// Writer durability level (`synchronous` pragma). Defaults to [`Synchronous::Normal`].
    pub synchronous: Synchronous,
    /// SQLite's `busy_timeout` (in milliseconds) for writer connections. Defaults to 5 seconds.
    pub busy_timeout: Duration,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        let reader_pool_size = get_available_cups();
        Self {
            reader_pool_size,
            synchronous: Synchronous::default(),
            busy_timeout: Duration::from_secs(5),
        }
    }
}

fn get_available_cups() -> usize {
    match thread::available_parallelism() {
        Ok(count) => count.into(),
        Err(_) => 4,
    }
}

pub struct SqliteStorageEngine {
    db: Database,
}

impl StorageEngine for SqliteStorageEngine {
    type Issuers = SqliteIssuerRepository;

    fn repositories(&self) -> Repositories<Self> {
        Repositories {
            issuers: SqliteIssuerRepository::new(self.db.handle()),
        }
    }

    fn dry_run<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Repositories<Self>) -> T,
    {
        self.db.dry_run(|handle| {
            f(&Repositories {
                issuers: SqliteIssuerRepository::new(handle.clone()),
            })
        })
    }
}

use std::cmp::min;
use std::thread;
use std::time::Duration;

use crate::sqlite::connection::pragmas::Synchronous;

/// Configuration for opening a [`Database`](crate::sqlite::connection::Database).
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
        Self {
            reader_pool_size: default_reader_pool_size(),
            synchronous: Synchronous::default(),
            busy_timeout: Duration::from_secs(5),
        }
    }
}

/// Best-effort available parallelism, used as the default reader-pool size.
fn default_reader_pool_size() -> usize {
    match thread::available_parallelism() {
        Ok(available_parallelism) => min(available_parallelism.into(), 6),
        Err(_) => 2,
    }
}

use std::cmp::min;
use std::thread;
use std::time::Duration;

use crate::sqlite::connection::pragmas::Synchronous;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub reader_pool_size: usize,
    pub synchronous: Synchronous,
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

fn default_reader_pool_size() -> usize {
    match thread::available_parallelism() {
        Ok(available_parallelism) => min(available_parallelism.into(), 6),
        Err(_) => 2,
    }
}

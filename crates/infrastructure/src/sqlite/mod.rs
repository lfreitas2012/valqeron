//! Sqlite-backed data access for the Valqeron infrastructure.
//!
//! Architecture invariants
//! 1. All processes using a database must be on the same host computer; WAL does not work over a network filesystem.
//! 2. Supported multi-process shape is **one writing process + any number of reading processes**.
//!    The app-level writer mutex serializes writers only *within* a process; concurrent writers
//!    across processes are not a supported/tested configuration (they would contend at SQLite's own
//!    write lock, bounded by `busy_timeout`).
//! 3. Writer durability defaults to `synchronous=NORMAL` (fast; the last commit may be lost on a
//!    power/OS crash, but the database is never corrupted). Opt into power-loss durability via
//!    `DatabaseConfig::synchronous = Synchronous::Full` (or `StorageConfig::durability`).
//!

pub mod driver;
pub mod mapping;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod repository;

pub use driver::{Database, DatabaseConfig, DbHandle, SqliteDataDriverError, Synchronous};
pub use repository::SqliteIssuerRepository;

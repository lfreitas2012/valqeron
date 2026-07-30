//! Sqlite-backed data access for the Valqeron infrastructure.
//!
//! Architecture invariants
//! 1. All processes using a database must be on the same host computer; WAL does not work over a network filesystem.
//! 

pub mod driver;
pub mod mapping;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod repository;

pub use driver::{Database, DatabaseConfig, DbHandle, SqliteDataDriverError};
pub use repository::SqliteIssuerRepository;

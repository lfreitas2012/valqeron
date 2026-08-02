//! The SQLite storage backend.
//!
//! The module tree is organized by responsibility:
//!
//! * [`connection`] — the connection layer (pooling, pragmas, guards, dry-run, the `Database`).
//! * [`engine`] — the [`SqliteStorageEngine`] adapter implementing the core `StorageEngine` port.
//! * [`issuer`] — issuer persistence (repository, SQL, row model, column converters).
//! * [`row`] — generic, entity-agnostic row-mapping primitives.
//! * [`migrations`] — the embedded schema migration runner.
//! * [`error`] — the SQLite driver error type.
//!
//! The public surface is deliberately small: the [`SqliteStorageEngine`] plus the
//! [`DatabaseConfig`], [`Synchronous`], and [`SqliteError`] types the application needs to open and
//! configure it.

mod connection;
mod engine;
mod error;
mod issuer;
mod migrations;
mod row;

pub use crate::sqlite::connection::{DatabaseConfig, Synchronous};
pub use crate::sqlite::engine::SqliteStorageEngine;
pub use crate::sqlite::error::SqliteDbError as SqliteError;

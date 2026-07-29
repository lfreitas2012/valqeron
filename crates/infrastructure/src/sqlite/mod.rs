//! SQLite storage implementation, organized into layered modules:
//!
//! - [`driver`]: connection lifecycle — writer + reader pool, pragmas, dry-run.
//! - [`migrations`]: embedded schema migrations and version tracking.
//! - [`mapping`]: the [`mapping::FromRow`] trait and column conversion helpers.
//! - [`models`]: row newtypes that reconstitute domain objects from rows.
//! - [`queries`]: stateless SQL statements returning raw rusqlite results.
//! - [`repository`]: domain repository composing the above and mapping outcomes
//!   onto the domain [`RepositoryError`](valqeron_core::RepositoryError).

pub mod driver;
pub mod mapping;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod repository;

pub use driver::{Database, DatabaseConfig, DbHandle, SqliteDataDriverError};
pub use repository::SqliteIssuerRepository;

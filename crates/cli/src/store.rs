//! The CLI's persistence seam.
//!
//! This is the single place that knows the concrete storage backend (SQLite today). It opens the
//! engine and exposes a backend-agnostic [`Repos`] view over the repositories, so the command layer
//! and dispatcher depend only on the domain ports — never on `SqliteStorageEngine` or a specific
//! entity's repository. To swap backends, only this module changes.
//!
//! # Connection model (provided by the storage backend)
//!
//! The engine hides the backend and routes work to one of **three** connection modes; the CLI never
//! picks a connection directly, it only chooses whether to run inside a dry-run or against the live
//! store, and the backend selects the right connection per repository method.
//!
//! 1. **Write connection** — a single writer serialized behind a mutex, running in WAL mode and
//!    auto-committing each operation. Used by mutating methods (`insert`, `update`, `apply_patch`,
//!    `delete`).
//! 2. **Read-only pool** — a fixed pool of `query_only` connections. Under WAL these never block the
//!    writer or one another, giving real read concurrency. Used by read methods.
//! 3. **Dry-run** — rehearses the real write path on the writer connection inside a `SAVEPOINT` that
//!    is **always rolled back**. Engaged by the global `--dry-run` flag.

use valqeron_core::{IssuerRepository, PersistenceManager, Repositories, StorageEngine};
use valqeron_infrastructure::{DatabaseConfig, SqliteStorageEngine};

use crate::config::ValqeronConfig;
use crate::error::AppResult;

/// Open (or create) the store at the configured database path, applying any pending migrations.
///
/// This is the one site that names the concrete engine; the rest of the CLI works through the
/// returned [`PersistenceManager`] and the [`Repos`] view.
pub fn open(config: &ValqeronConfig) -> AppResult<PersistenceManager<SqliteStorageEngine>> {
    let db_config = DatabaseConfig {
        reader_pool_size: config.reader_pool_size(),
        synchronous: config.synchronous(),
        ..DatabaseConfig::default()
    };
    let engine = SqliteStorageEngine::open(config.db_path(), db_config)?;
    Ok(PersistenceManager::new(engine))
}

/// A borrowed, backend-agnostic view over the engine's repositories.
///
/// Commands receive this and pull the specific port(s) they need (e.g. [`Repos::issuers`]). Adding a
/// new entity means adding a field and accessor here plus one line in [`repos`] — the dispatcher and
/// the `Command` trait are untouched.
pub struct Repos<'a> {
    issuers: &'a dyn IssuerRepository,
}

impl Repos<'_> {
    /// The issuer repository port.
    pub fn issuers(&self) -> &dyn IssuerRepository {
        self.issuers
    }
}

/// Build the backend-agnostic [`Repos`] view from an engine's concrete repository set.
///
/// Generic over the engine so it serves both the live path ([`PersistenceManager::repositories`])
/// and the dry-run path (the `&Repositories<E>` handed to the dry-run closure).
pub fn repos<E: StorageEngine>(repositories: &Repositories<E>) -> Repos<'_> {
    Repos {
        issuers: &repositories.issuers,
    }
}

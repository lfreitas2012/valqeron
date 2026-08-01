//! Thin server layer over `valqeron-core`.
//!
//! Today this is an in-process facade that wraps [`PersistenceManager`]. It
//! exists to draw a stable boundary: everything above it (commands, I/O,
//! rendering) depends only on [`Server`], never on `Store` directly. When
//! Valqeron grows a real `valqeron-server` daemon (systemd/launchd) with an IPC
//! client, only this module changes — the command layer is untouched.
//!
//! # Connection model (provided by the storage backend)
//!
//! The `Store` hides the backend (SQLite today) and routes work to one of
//! **three** connection modes. The CLI never picks a connection directly; it
//! only chooses whether to run inside a dry-run or against the live store, and
//! the backend selects the right connection per repository method.
//!
//! 1. **Write connection** — a single writer serialized behind a mutex, running
//!    in WAL mode and auto-committing each operation. Used by mutating methods
//!    (`insert`, `update`, `apply_patch`, `delete`). Reached via
//!    [`Server::with_issuers`].
//!
//! 2. **Read-only pool** — a fixed pool of `query_only` connections (size =
//!    [`ValqeronConfig::reader_pool_size`](ValqeronConfig::reader_pool_size)).
//!    Under WAL these never block the writer or one another, giving real read
//!    concurrency. Used by read methods (`find_by_id`, `exists`), also reached
//!    via [`Server::with_issuers`].
//!
//! 3. **Dry-run** — rehearses the real write path on the writer connection inside
//!    a `SAVEPOINT` that is **always rolled back**. It holds the writer lock for
//!    the whole operation (so concurrent writers queue behind it rather than
//!    racing at the SQLite layer), fires all validation and constraint checks, but
//!    persists nothing. Reached via [`Server::dry_run`], engaged by the global
//!    `--dry-run` flag.

use valqeron_core::{IssuerRepository, PersistenceManager};

use crate::config::ValqeronConfig;
use crate::error::AppResult;

/// An opened Valqeron store, ready to serve repository work.
pub struct Server {
    store: PersistenceManager,
}

impl Server {
    /// Open (or create) the store at the configured database path, applying
    /// any pending schema migrations.
    pub fn open(config: &ValqeronConfig) -> AppResult<Self> {
        let storage_config = StorageConfig {
            reader_pool_size: config.reader_pool_size(),
            durability: config.durability(),
        };
        let store = open_sqlite(config.db_path(), storage_config)?;
        Ok(Self { store })
    }

    /// Run `f` against the live issuer repository.
    ///
    /// Reads are served from the reader pool; writes go through the serialized
    /// writer and auto-commit. Use this for the normal (non-dry-run) path.
    pub fn with_issuers<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&dyn IssuerRepository) -> AppResult<T>,
    {
        // `Store::issuers` hands back a boxed repository; deref to a `&dyn` so
        // the command closures stay backend-agnostic.
        let repo = self.store.issuers_repository();
        f(&*repo)
    }

    /// Run `f` against an isolated dry-run repository whose writes are always
    /// rolled back. The closure's `Result` is returned unchanged; whether it
    /// succeeded or failed, nothing is persisted.
    pub fn dry_run<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&dyn IssuerRepository) -> AppResult<T>,
    {
        // `Store::dry_run` maps backend errors into `StorageError`; the inner
        // `AppResult<T>` is passed straight through and then flattened.
        self.store.dry_run(|repo| f(repo))?
    }
}

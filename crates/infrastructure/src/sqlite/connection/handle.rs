//! Repository access to SQLite connections.
//!
//! Repositories use [`DbHandle`] through [`Db`], which supports both live operation and dry-runs
//! without a second repository implementation.

use crate::sqlite::connection::dry_run::current_dry_run_conn;
use crate::sqlite::connection::guard::{ReadGuard, WriteGuard};
use crate::sqlite::connection::pool::{ReaderPool, ReaderSource, lock_writer};
use crate::sqlite::connection::pragmas::SharedConnection;

/// Provides read and write access to SQLite connections.
///
/// Implemented by [`DbHandle`] in both its operating modes. Repositories depend on this trait, so
/// the same code path serves real and dry-run work.
pub(crate) trait Db {
    /// Acquire the writer connection. Use for any statement that mutates data.
    fn write(&self) -> WriteGuard<'_>;

    /// Acquire a read connection.
    fn read(&self) -> ReadGuard<'_>;
}

/// A cloneable handle used by repositories to access SQLite.
///
/// A single repository type is generic over [`Db`], and this one-handle type serves both operating
/// modes so the storage engine can expose a single concrete repository:
///
/// * [`DbHandle::Live`] uses pooled read connections and one mutex-guarded writer connection.
/// * [`DbHandle::DryRun`] routes all operations to the connection pinned by
///   [`Database::dry_run`](crate::sqlite::connection::Database::dry_run).
#[derive(Clone)]
pub(crate) enum DbHandle {
    Live {
        writer: SharedConnection,
        readers: ReaderSource,
    },
    DryRun,
}

impl Db for DbHandle {
    /// Acquires the writer connection.
    ///
    /// In [`DbHandle::Live`] mode this locks the single writer connection (serialized via mutex).
    /// A poisoned writer mutex is recovered, and any stranded transaction is rolled back; see
    /// [`lock_writer`].
    ///
    /// In [`DbHandle::DryRun`] mode it borrows the thread-pinned dry-run connection (already locked
    /// by [`Database::dry_run`](crate::sqlite::connection::Database::dry_run) for the whole closure).
    fn write(&self) -> WriteGuard<'_> {
        let guard = match self {
            DbHandle::Live { writer, .. } => WriteGuard::Locked(lock_writer(writer)),
            DbHandle::DryRun => WriteGuard::Borrowed(current_dry_run_conn()),
        };
        guard.start_operation();
        guard
    }

    /// Acquires a read connection.
    ///
    /// In [`DbHandle::Live`] mode this checks a connection out of the reader pool (blocking if all
    /// are in use), or shares the writer for in-memory databases. In [`DbHandle::DryRun`] mode it
    /// borrows the thread-pinned dry-run connection so reads observe the dry-run's own uncommitted
    /// writes.
    fn read(&self) -> ReadGuard<'_> {
        let guard = match self {
            DbHandle::Live { writer, readers } => match readers {
                ReaderSource::Pool(pool) => ReadGuard::Pooled(ReaderPool::checkout(pool)),
                ReaderSource::SharedWithWriter => ReadGuard::Locked(lock_writer(writer)),
            },
            DbHandle::DryRun => ReadGuard::Borrowed(current_dry_run_conn()),
        };
        guard.start_operation();
        guard
    }
}

//! The connection-source abstraction handed to repositories.
//!
//! A single repository type ([`SqliteIssuerRepository`](crate::sqlite::issuer::repository::SqliteIssuerRepository))
//! holds a [`DbHandle`] and reaches the database only through the [`Db`] trait, so the same code
//! serves the normal path and a dry-run without opening a second connection.

use crate::sqlite::connection::dry_run::current_dry_run_conn;
use crate::sqlite::connection::guard::{ReadGuard, WriteGuard};
use crate::sqlite::connection::pool::{ReaderPool, ReaderSource, lock_writer};
use crate::sqlite::connection::pragmas::SharedConnection;

/// A source of database connections for repositories.
///
/// Implemented by [`DbHandle`] in both its operating modes. Repositories depend on this trait so the
/// same code path serves real and dry-run work.
pub(crate) trait Db {
    /// Acquire the write connection. Use for any statement that mutates data.
    fn write(&self) -> WriteGuard<'_>;

    /// Acquire a read connection.
    fn read(&self) -> ReadGuard<'_>;
}

/// A shared, cloneable handle used by repositories to reach the database.
///
/// A single repository type is generic over [`Db`], and this one handle type serves both operating
/// modes so the storage engine can expose a single concrete repository:
///
/// * [`DbHandle::Live`] — the normal path. Reads are served from a pool of `query_only`
///   connections; writes are serialized through a single writer connection guarded by a mutex.
/// * [`DbHandle::DryRun`] — used only inside
///   [`Database::dry_run`](crate::sqlite::connection::Database::dry_run). Every read/write is routed
///   to the single connection the dry-run pinned for this thread, so all work runs inside the
///   dry-run `SAVEPOINT` without re-locking the writer mutex.
#[derive(Clone)]
pub(crate) enum DbHandle {
    Live {
        writer: SharedConnection,
        readers: ReaderSource,
    },
    DryRun,
}

impl Db for DbHandle {
    /// Access the write connection.
    ///
    /// In [`DbHandle::Live`] mode this locks the single writer connection (serialized via mutex).
    /// Healing note: if a prior writer panicked mid-transaction and poisoned the mutex, the guard is
    /// recovered and any stranded transaction is rolled back before it is handed back — see
    /// [`lock_writer`].
    ///
    /// In [`DbHandle::DryRun`] mode it borrows the thread-pinned dry-run connection (already locked
    /// by [`Database::dry_run`](crate::sqlite::connection::Database::dry_run) for the whole closure).
    fn write(&self) -> WriteGuard<'_> {
        match self {
            DbHandle::Live { writer, .. } => WriteGuard::Locked(lock_writer(writer)),
            DbHandle::DryRun => WriteGuard::Borrowed(current_dry_run_conn()),
        }
    }

    /// Acquire a read connection.
    ///
    /// In [`DbHandle::Live`] mode this checks a connection out of the reader pool (blocking if all
    /// are in use), or shares the writer for in-memory databases. In [`DbHandle::DryRun`] mode it
    /// borrows the thread-pinned dry-run connection so reads observe the dry-run's own uncommitted
    /// writes.
    fn read(&self) -> ReadGuard<'_> {
        match self {
            DbHandle::Live { writer, readers } => match readers {
                ReaderSource::Pool(pool) => ReadGuard::Pooled(ReaderPool::checkout(pool)),
                ReaderSource::SharedWithWriter => ReadGuard::Locked(lock_writer(writer)),
            },
            DbHandle::DryRun => ReadGuard::Borrowed(current_dry_run_conn()),
        }
    }
}

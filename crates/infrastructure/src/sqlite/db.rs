use crate::sqlite::driver::{DbPath, ReaderPool, ReaderSource, SharedConnection, lock_writer};
use crate::sqlite::error::SqliteDbError;
use crate::sqlite::migrations;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub up: fn(&rusqlite::Transaction) -> Result<(), Box<dyn std::error::Error + Send + Sync>>,
}

pub trait MigrationSource {
    fn migrations(&self) -> &[Migration];
}

/// A read guard: either a pooled reader connection (normal operation) or a borrowed connection
/// (inside a dry-run, where all work shares the already-locked writer connection).
pub enum ReadGuard<'a> {
    Pooled(PooledReader),
    Locked(MutexGuard<'a, Connection>),
    Borrowed(&'a Connection),
}

impl std::ops::Deref for ReadGuard<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        match self {
            ReadGuard::Pooled(p) => p,
            ReadGuard::Locked(g) => g,
            ReadGuard::Borrowed(c) => c,
        }
    }
}

/// A write guard: either the mutex-locked writer connection (normal operation) or a borrowed
/// connection (inside a dry-run, where the writer mutex is already held by the dry-run driver).
pub enum WriteGuard<'a> {
    Locked(MutexGuard<'a, Connection>),
    Borrowed(&'a Connection),
}

impl std::ops::Deref for WriteGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            WriteGuard::Locked(g) => g,
            WriteGuard::Borrowed(c) => c,
        }
    }
}

/// RAII handle to a checked-out read connection. Returns the connection to the pool on a drop.
pub struct PooledReader {
    pub(crate) pool: Arc<ReaderPool>,
    pub(crate) conn: Option<Connection>,
}

impl Drop for PooledReader {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.checkin(conn);
        }
    }
}

impl std::ops::Deref for PooledReader {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

/// A source of database connections for repositories.
///
/// Implemented by both [`DbHandle`] (the normal pooled/serialized path) and [`DryRunHandle`] (a
/// borrowed connection running inside a dry-run transaction). Repositories are generic over this
/// trait so the same code path serves real and dry-run work without a second physical connection.
pub trait Db {
    /// Acquire the write connection. Use for any statement that mutates data.
    fn write(&self) -> WriteGuard<'_>;

    /// Acquire a read connection.
    fn read(&self) -> ReadGuard<'_>;
}

/// A shared handle used by repositories to reach the database.
///
/// Reads are served from a pool of `query_only` connections; writes are serialized through a
/// single writer connection guarded by a mutex.
#[derive(Clone)]
pub struct DbHandle {
    writer: SharedConnection,
    readers: ReaderSource,
}

/// Owns the writer connection and reader pool. The entry point for opening a database and running
/// dry-runs.
pub struct Database {
    handle: DbHandle,
    path: DbPath,
}

impl Db for DbHandle {
    /// Access the single writer connection (serialized via mutex). Use for any statement that mutates data.
    ///
    /// Healing note: if a prior writer panicked mid-transaction and poisoned the mutex, the guard is
    /// recovered and any stranded transaction is rolled back before it is handed back — see
    /// [`lock_writer`].
    fn write(&self) -> WriteGuard<'_> {
        WriteGuard::Locked(lock_writer(&self.writer))
    }

    /// Check a read connection out of the pool, blocking if all are in use.
    fn read(&self) -> ReadGuard<'_> {
        match &self.readers {
            ReaderSource::Pool(pool) => ReadGuard::Pooled(ReaderPool::checkout(pool)),
            ReaderSource::SharedWithWriter => ReadGuard::Locked(lock_writer(&self.writer)),
        }
    }
}

/// A borrowed connection source used only inside [`Database::dry_run`].
///
/// Both `write()` and `read()` return the *same* borrowed writer connection, which the dry-run
/// driver keeps locked for the whole closure. This makes every operation run inside the dry-run's
/// `SAVEPOINT` (so reads observe uncommitted writes) without re-locking the writer mutex, which
/// would otherwise self-deadlock.
#[derive(Clone, Copy)]
pub struct DryRunHandle<'a> {
    conn: &'a Connection,
}

impl Db for DryRunHandle<'_> {
    fn write(&self) -> WriteGuard<'_> {
        WriteGuard::Borrowed(self.conn)
    }

    fn read(&self) -> ReadGuard<'_> {
        ReadGuard::Borrowed(self.conn)
    }
}

impl Database {
    /// Open (or create) a database at `path` with the default configuration.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteDataDriverError> {
        Self::open_with_config(path, DatabaseConfig::default())
    }

    /// Open (or create) a database at `path` with the given configuration.
    pub fn open_with_config(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, SqliteDataDriverError> {
        let path = DbPath::File(path.as_ref().to_path_buf());
        Self::open_inner(path, config)
    }

    /// Open an isolated in-memory database (default configuration). Backed by a uniquely named shared
    /// cache, so the writer and reader-pool connections all observe the same data.
    pub fn open_in_memory() -> Result<Self, SqliteDataDriverError> {
        Self::open_in_memory_with_config(DatabaseConfig::default())
    }

    /// Open an isolated in-memory database with the given configuration. Backed by a uniquely
    /// named `memdb`-VFS database, so the writer and reader-pool connections all observe the same
    /// data without relying on SQLite's shared-cache mode.
    pub fn open_in_memory_with_config(
        config: DatabaseConfig,
    ) -> Result<Self, SqliteDataDriverError> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("valqeron-mem-{}-{n}", std::process::id());
        Self::open_inner(DbPath::Memory(name), config)
    }

    fn open_inner(path: DbPath, config: DatabaseConfig) -> Result<Self, SqliteDataDriverError> {
        if config.reader_pool_size < 1 {
            return Err(SqliteDataDriverError::InvalidPoolSize);
        }

        let is_memory = matches!(path, DbPath::Memory(_));

        let mut writer = crate::sqlite::driver::open_connection(
            &path,
            crate::sqlite::driver::ConnectionRole::Writer,
        )?;
        crate::sqlite::driver::configure(
            &writer,
            crate::sqlite::driver::ConnectionRole::Writer,
            is_memory,
            config.synchronous,
        )?;
        migrations::run(&mut writer)?;

        let readers = if is_memory {
            ReaderSource::SharedWithWriter
        } else {
            let mut pool = Vec::with_capacity(config.reader_pool_size);
            for _ in 0..config.reader_pool_size {
                let conn = crate::sqlite::driver::open_connection(
                    &path,
                    crate::sqlite::driver::ConnectionRole::Reader,
                )?;
                crate::sqlite::driver::configure(
                    &conn,
                    crate::sqlite::driver::ConnectionRole::Reader,
                    is_memory,
                    config.synchronous,
                )?;
                pool.push(conn);
            }
            ReaderSource::Pool(Arc::new(ReaderPool::new(pool)))
        };

        Ok(Self {
            handle: DbHandle {
                writer: Arc::new(Mutex::new(writer)),
                readers,
            },
            path,
        })
    }

    /// An inexpensive, cloneable handle to hand to repositories. Clones share the same writer
    /// and reader pool.
    pub fn handle(&self) -> DbHandle {
        self.handle.clone()
    }

    /// Run `f` against a dry-run view of the database: every write it performs is rolled back on
    /// return and never persisted.
    ///
    /// Unlike a naive second-connection approach, this reuses the real writer connection: it holds
    /// the writer mutex for the whole closure (so concurrent real writes queue behind it at the
    /// app level, exactly as [`DbHandle`] promises, instead of racing for SQLite's write lock) and
    /// wraps the work in a `SAVEPOINT`. The closure is handed a [`DryRunHandle`] whose `write()` and
    /// `read()` both borrow this already-locked connection, so its reads observe its own uncommitted
    /// writes without re-locking the mutex (which would self-deadlock).
    pub fn dry_run<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&DryRunHandle<'_>) -> T,
    {
        // Hold the real writer guard for the entire dry-run. This serializes against every other
        // writer via the app-level mutex, so no concurrent write can slip a committed transaction
        // *inside* our savepoint and get rolled back with us.
        let guard = lock_writer(&self.handle.writer);

        guard
            .execute_batch("SAVEPOINT valqeron_dry_run")?;

        let handle = DryRunHandle { conn: &guard };
        let result = f(&handle);

        // Discard everything done inside the savepoint. ROLLBACK TO rewinds the changes; RELEASE
        // then pops the (now-empty) savepoint so the connection returns to autocommit.
        if let Err(e) =
            guard.execute_batch("ROLLBACK TO valqeron_dry_run; RELEASE valqeron_dry_run")
        {
            tracing::error!(
                error = %e,
                "dry-run savepoint rollback/release failed; attempting a plain RELEASE"
            );
            if let Err(e) = guard.execute_batch("RELEASE valqeron_dry_run") {
                tracing::error!(error = %e, "dry-run savepoint RELEASE also failed");
            }
        }

        Ok(result)
    }
}

/// Ownership note: this cleanup (`PRAGMA optimize` + WAL `TRUNCATE`) fires when the owning
/// [`Database`] value drops, NOT when the last cloned [`DbHandle`] drops. That is correct under the
/// current model, where a single long-lived `Database` owns the process and outlives every handle it
/// hands out. If a caller ever drops the `Database` early while cloned handles are still circulating
/// and writing, this would run a premature checkpoint mid-write, at that point switch to a
/// reference-counted close (e.g., cleanup on the last `Arc` drop) instead.
impl Drop for Database {
    fn drop(&mut self) {
        let conn = self.handle.write();
        if let Err(e) = conn.execute_batch("PRAGMA optimize;") {
            tracing::warn!(error = %e, "PRAGMA optimize failed on close");
        }

        if matches!(self.path, DbPath::File(_))
            && let Err(e) = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
        {
            tracing::warn!(error = %e, "WAL checkpoint(TRUNCATE) failed on close");
        }
    }
}

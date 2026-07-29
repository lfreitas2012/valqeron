//! SQLite storage driver.
//!
//! Concurrency model: a single writer connection serialized behind a mutex plus
//! a fixed pool of `query_only` reader connections. On disk this runs in WAL
//! mode, so readers never block the writer or each other. A dry-run executes on
//! its own isolated connection and is always rolled back, so it never affects
//! writes on other threads.

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

pub mod error;
pub mod models;
pub mod queries;

const MIGRATIONS: &[&str] = &[include_str!(
    "../../../migrations/001_create_initial_issuer_schema.sql"
)];

/// Default number of read connections in the pool.
pub const DEFAULT_READER_POOL_SIZE: usize = 4;

/// A connection shared behind a mutex; used for the serialized writer.
pub type SharedConnection = Arc<Mutex<Connection>>;

/// Errors from opening, configuring, migrating, or running a dry-run on the database.
#[derive(Debug, thiserror::Error)]
pub enum SqliteDataDriverError {
    #[error("failed to open sqlite connection")]
    Connection {
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to configure connection")]
    Pragma {
        #[source]
        source: rusqlite::Error,
    },

    #[error("migration failed")]
    Migration {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error(
        "database schema version {found} is newer than the {known} migration(s) this binary knows about — upgrade the binary"
    )]
    UnknownSchemaVersion { found: i64, known: usize },

    #[error("failed to open dry-run transaction")]
    DryRun {
        #[source]
        source: rusqlite::Error,
    },

    #[error("reader pool size must be at least 1")]
    InvalidPoolSize,
}

/// Configuration for opening a [`Database`].
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Number of read-only connections held in the reader pool.
    pub reader_pool_size: usize,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            reader_pool_size: DEFAULT_READER_POOL_SIZE,
        }
    }
}

/// Poison-tolerant lock helper: if another thread panicked while holding the lock, we recover the
/// guard rather than propagating the panic.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A fixed-size pool of read-only SQLite connections.
///
/// Under WAL, readers never block the single writer nor each other, so a pool of independent read
/// connections gives real read concurrency. Connections are checked out and returned via
/// [`PooledReader`]; when all are in use, callers block on the [`Condvar`] until one is returned.
pub(crate) struct ReaderPool {
    idle: Mutex<Vec<Connection>>,
    available: Condvar,
}

impl ReaderPool {
    fn new(connections: Vec<Connection>) -> Self {
        Self {
            idle: Mutex::new(connections),
            available: Condvar::new(),
        }
    }

    /// Check out a reader, blocking until one becomes available.
    fn checkout(self: &Arc<Self>) -> PooledReader {
        let mut idle = lock(&self.idle);
        loop {
            if let Some(conn) = idle.pop() {
                return PooledReader {
                    pool: Arc::clone(self),
                    conn: Some(conn),
                };
            }
            idle = self
                .available
                .wait(idle)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn checkin(&self, conn: Connection) {
        lock(&self.idle).push(conn);
        self.available.notify_one();
    }
}

/// RAII handle to a checked-out read connection. Returns the connection to the pool on a drop.
pub(crate) struct PooledReader {
    pool: Arc<ReaderPool>,
    conn: Option<Connection>,
}

impl std::ops::Deref for PooledReader {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl Drop for PooledReader {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.checkin(conn);
        }
    }
}

/// A read guard: either a pooled reader connection (normal operation) or the writer connection
/// itself (inside a dry-run, where all work shares one private connection).
pub(crate) enum ReadGuard<'a> {
    Pooled(PooledReader),
    Writer(MutexGuard<'a, Connection>),
}

impl std::ops::Deref for ReadGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            ReadGuard::Pooled(p) => p,
            ReadGuard::Writer(w) => w,
        }
    }
}

/// A shared handle used by repositories to reach the database.
///
/// Reads are served from a pool of `query_only` connections; writes are serialized through a
/// single writer connection guarded by a mutex.
#[derive(Clone)]
pub struct DbHandle {
    writer: SharedConnection,
    readers: Option<Arc<ReaderPool>>,
}

impl DbHandle {
    /// Access the single writer connection (serialized via mutex). Use for any statement that
    /// mutates data.
    pub(crate) fn write(&self) -> MutexGuard<'_, Connection> {
        lock(&self.writer)
    }

    /// Acquire a read connection. In normal operation this checks one out from the pool; inside a
    /// dry-run (no pool) reads run on the writer connection so they observe the dry-run
    /// transaction's uncommitted state.
    pub(crate) fn read(&self) -> ReadGuard<'_> {
        match &self.readers {
            Some(pool) => ReadGuard::Pooled(pool.checkout()),
            None => ReadGuard::Writer(lock(&self.writer)),
        }
    }
}

/// Owns the writer connection and reader pool. The entry point for opening a database and running
/// dry-runs.
pub struct Database {
    handle: DbHandle,
    path: DbPath,
}

/// Where the database lives; needed to open the isolated dry-run connection.
#[derive(Clone)]
enum DbPath {
    File(PathBuf),
    /// A named shared-cache in-memory database (survives across connections for the lifetime of
    /// at least one open handle).
    SharedMemory(String),
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
    /// cache so the writer, reader pool, and dry-run connections all observe the same data.
    pub fn open_in_memory() -> Result<Self, SqliteDataDriverError> {
        Self::open_in_memory_with_config(DatabaseConfig::default())
    }

    /// Open an isolated in-memory database with the given configuration.
    pub fn open_in_memory_with_config(
        config: DatabaseConfig,
    ) -> Result<Self, SqliteDataDriverError> {
        // A process-unique name keeps independent in-memory databases isolated
        // from one another while still shared across this handle's connections.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("valqeron-mem-{}-{n}", std::process::id());
        Self::open_inner(DbPath::SharedMemory(name), config)
    }

    fn open_inner(path: DbPath, config: DatabaseConfig) -> Result<Self, SqliteDataDriverError> {
        if config.reader_pool_size < 1 {
            return Err(SqliteDataDriverError::InvalidPoolSize);
        }

        let is_memory = matches!(path, DbPath::SharedMemory(_));

        // Writer: run migrations here, once, before readers are usable.
        let mut writer = open_connection(&path, ConnectionRole::Writer)?;
        configure(&writer, ConnectionRole::Writer, is_memory)?;
        run_migrations(&mut writer)?;

        // Readers: independent connections, query-only.
        let mut readers = Vec::with_capacity(config.reader_pool_size);
        for _ in 0..config.reader_pool_size {
            let conn = open_connection(&path, ConnectionRole::Reader)?;
            configure(&conn, ConnectionRole::Reader, is_memory)?;
            readers.push(conn);
        }

        Ok(Self {
            handle: DbHandle {
                writer: Arc::new(Mutex::new(writer)),
                readers: Some(Arc::new(ReaderPool::new(readers))),
            },
            path,
        })
    }

    /// An inexpensive, cloneable handle to hand to repositories. Clones share the same writer
    /// and reader pool.
    pub fn handle(&self) -> DbHandle {
        self.handle.clone()
    }

    /// Run `f` inside a dry-run: `f` receives a [`DbHandle`] bound to a private, isolated connection
    /// with an open transaction. Whatever `f` does is **always rolled back** and never persisted,
    /// and, because it uses its own connection, it never affects writes happening on other threads.
    ///
    /// The dry-run holds SQLite's write lock (`BEGIN IMMEDIATE`) for its duration; concurrent writers
    /// (including other processes) wait up to `busy_timeout`.
    ///
    /// The rollback is best-effort but safe: even if the explicit `ROLLBACK` errored, dropping the
    /// private connection discards any uncommitted transaction, so nothing can leak into the
    /// persisted database.
    pub fn dry_run<F, T>(&self, f: F) -> Result<T, SqliteDataDriverError>
    where
        F: FnOnce(&DbHandle) -> T,
    {
        let is_memory = matches!(self.path, DbPath::SharedMemory(_));
        let conn = open_connection(&self.path, ConnectionRole::Writer)?;
        configure(&conn, ConnectionRole::Writer, is_memory)?;

        // Use a plain statement rather than the RAII `Transaction` type so the
        // repository can keep operating on a `Connection`. BEGIN IMMEDIATE takes
        // the write lock up front for consistent isolation.
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|source| SqliteDataDriverError::DryRun { source })?;

        // The closure's handle points its writer and readers at this single
        // private connection, so every op runs inside the transaction.
        let shared: SharedConnection = Arc::new(Mutex::new(conn));
        let handle = DbHandle {
            writer: Arc::clone(&shared),
            readers: None,
        };

        let result = f(&handle);

        // Reclaim the connection (the handle holds the only other Arc; drop it).
        drop(handle);
        let conn = Arc::try_unwrap(shared)
            .map(|m| m.into_inner().unwrap_or_else(|p| p.into_inner()))
            .expect("dry-run connection is uniquely owned after the closure returns");

        if let Err(e) = conn.execute_batch("ROLLBACK") {
            tracing::error!(error = %e, "dry-run explicit ROLLBACK failed; connection drop will discard the transaction");
        }
        // `conn` drops here, discarding any lingering transaction as a backstop.

        Ok(result)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let conn = self.handle.write();
        if let Err(e) = conn.execute_batch("PRAGMA optimize;") {
            tracing::warn!(error = %e, "PRAGMA optimize failed on close");
        }
        // Truncate the WAL so the -wal file does not grow unbounded across
        // long-lived sessions while another process may still be reading.
        // (Only meaningful for on-disk WAL databases.)
        if matches!(self.path, DbPath::File(_))
            && let Err(e) = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
        {
            tracing::warn!(error = %e, "WAL checkpoint(TRUNCATE) failed on close");
        }
    }
}

#[derive(Clone, Copy)]
enum ConnectionRole {
    Writer,
    Reader,
}

fn open_connection(
    path: &DbPath,
    role: ConnectionRole,
) -> Result<Connection, SqliteDataDriverError> {
    let map_err = |source| SqliteDataDriverError::Connection { source };
    match path {
        DbPath::File(p) => Connection::open(p).map_err(map_err),
        DbPath::SharedMemory(name) => {
            let uri = format!("file:{name}?mode=memory&cache=shared");
            let flags = match role {
                ConnectionRole::Writer => {
                    OpenFlags::SQLITE_OPEN_READ_WRITE
                        | OpenFlags::SQLITE_OPEN_CREATE
                        | OpenFlags::SQLITE_OPEN_URI
                }
                ConnectionRole::Reader => {
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI
                }
            };
            Connection::open_with_flags(uri, flags).map_err(map_err)
        }
    }
}

fn configure(
    conn: &Connection,
    role: ConnectionRole,
    is_memory: bool,
) -> Result<(), SqliteDataDriverError> {
    let pragma_err = |source| SqliteDataDriverError::Pragma { source };

    // In-memory shared-cache databases do not support WAL; skip journal-mode and WAL-autocheckpoint there.
    // On-disk databases use WAL for concurrent readers.
    if !is_memory {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(pragma_err)?;
    }

    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(pragma_err)?;

    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(pragma_err)?;

    conn.busy_timeout(Duration::from_secs(5))
        .map_err(pragma_err)?;

    conn.pragma_update(None, "cache_size", -64_000i64)
        .map_err(pragma_err)?;

    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(pragma_err)?;

    conn.pragma_update(None, "mmap_size", 256i64 * 1024 * 1024)
        .map_err(pragma_err)?;

    if !is_memory {
        conn.pragma_update(None, "wal_autocheckpoint", 1000i64)
            .map_err(pragma_err)?;
    }

    conn.set_prepared_statement_cache_capacity(64);

    if let ConnectionRole::Reader = role {
        conn.pragma_update(None, "query_only", "ON")
            .map_err(pragma_err)?;
    }

    Ok(())
}

fn run_migrations(connection: &mut Connection) -> Result<(), SqliteDataDriverError> {
    fn migration_err(
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> SqliteDataDriverError {
        SqliteDataDriverError::Migration {
            source: Box::new(source),
        }
    }

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(migration_err)?;

    if current_version as usize > MIGRATIONS.len() {
        return Err(SqliteDataDriverError::UnknownSchemaVersion {
            found: current_version,
            known: MIGRATIONS.len(),
        });
    }

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let migration_version = (index + 1) as i64;
        if migration_version <= current_version {
            continue;
        }

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(migration_err)?;

        tx.execute_batch(sql).map_err(migration_err)?;

        tx.pragma_update(None, "user_version", migration_version)
            .map_err(migration_err)?;

        tx.commit().map_err(migration_err)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn insert_dummy(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO issuer (id, status, created_at)
             VALUES (randomblob(16), 'ACTIVE', '2026-01-01T00:00:00Z')",
        )
        .unwrap();
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM issuer", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_database_ends_up_at_latest_version() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();
        let conn = handle.read();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn opening_twice_is_a_harmless_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("twice.db");
        let _db1 = Database::open(&path).unwrap();
        drop(_db1);
        // Reopening runs migrations again over an already-migrated file.
        let _db2 = Database::open(&path).unwrap();
    }

    #[test]
    fn schema_from_the_future_is_rejected_rather_than_silently_skipped() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure(&conn, ConnectionRole::Writer, true).unwrap();

        conn.pragma_update(None, "user_version", (MIGRATIONS.len() as i64) + 5)
            .unwrap();

        let result = run_migrations(&mut conn);
        assert!(matches!(
            result,
            Err(SqliteDataDriverError::UnknownSchemaVersion { .. })
        ));
    }

    #[test]
    fn invalid_pool_size_is_rejected() {
        let result = Database::open_in_memory_with_config(DatabaseConfig {
            reader_pool_size: 0,
        });
        assert!(matches!(
            result,
            Err(SqliteDataDriverError::InvalidPoolSize)
        ));
    }

    #[test]
    fn dry_run_rolls_back_writes() {
        let db = Database::open_in_memory().unwrap();

        db.dry_run(|h| {
            insert_dummy(&h.write());
            assert_eq!(count(&h.read()), 1, "write visible inside the dry-run");
        })
        .unwrap();

        // After the dry-run, nothing persisted.
        assert_eq!(count(&db.handle().read()), 0);
    }

    #[test]
    fn dry_run_returns_closure_value() {
        let db = Database::open_in_memory().unwrap();
        let n = db
            .dry_run(|h| {
                insert_dummy(&h.write());
                count(&h.read())
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn dry_run_does_not_roll_back_committed_writes_on_other_connections() {
        // The core correctness guarantee: a dry-run on its own connection must
        // NOT discard real writes made concurrently on the main connection.
        let db = Database::open_in_memory().unwrap();

        // Commit a real write on the main writer.
        insert_dummy(&db.handle().write());
        assert_eq!(count(&db.handle().read()), 1);

        // Run a dry-run that also writes, then rolls back.
        db.dry_run(|h| {
            insert_dummy(&h.write());
            assert_eq!(count(&h.read()), 2, "sees its own + the committed row");
        })
        .unwrap();

        // The real write survives; only the dry-run write was discarded.
        assert_eq!(count(&db.handle().read()), 1);
    }

    #[test]
    fn without_a_dry_run_writes_persist_normally() {
        let db = Database::open_in_memory().unwrap();
        insert_dummy(&db.handle().write());
        assert_eq!(count(&db.handle().read()), 1);
    }

    #[test]
    fn concurrent_reads_are_served_while_a_write_lock_is_held() {
        let db = Database::open_in_memory_with_config(DatabaseConfig {
            reader_pool_size: 2,
        })
        .unwrap();
        let handle = db.handle();

        // Hold the writer lock on this thread.
        let write_guard = handle.write();

        // A reader on another thread must still make progress (pool of readers).
        let h2 = handle.clone();
        let done = thread::spawn(move || {
            let r = h2.read();
            count(&r)
        });

        let n = done.join().unwrap();
        assert_eq!(n, 0);
        drop(write_guard);
    }

    #[test]
    fn reader_pool_blocks_then_resumes_when_exhausted() {
        let db = Database::open_in_memory_with_config(DatabaseConfig {
            reader_pool_size: 1,
        })
        .unwrap();
        let handle = db.handle();

        let counter = Arc::new(AtomicUsize::new(0));

        // Check out the only reader and hold it.
        let held = handle.read();

        let h2 = handle.clone();
        let c2 = Arc::clone(&counter);
        let waiter = thread::spawn(move || {
            let _r = h2.read(); // blocks until `held` is returned
            c2.fetch_add(1, Ordering::SeqCst);
        });

        // Give the waiter a chance to block.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "waiter should be blocked"
        );

        drop(held); // return the reader → waiter unblocks
        waiter.join().unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn two_handles_on_one_file_see_each_others_committed_writes() {
        // Simulates CLI + Desktop (two Database handles / connections on one file).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.db");

        let db_a = Database::open(&path).unwrap();
        let db_b = Database::open(&path).unwrap();

        insert_dummy(&db_a.handle().write());

        // Reader in the second "process" sees the committed row (WAL visibility).
        assert_eq!(count(&db_b.handle().read()), 1);
    }

    #[test]
    fn shared_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Database>();
        assert_send_sync::<DbHandle>();
        assert_send_sync::<Arc<ReaderPool>>();
    }
}

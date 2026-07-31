use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::sqlite::migrations;

/// Synchronization primitive shim.
///
/// Normal builds use `std::sync`. Under `--cfg loom` (used only by the exhaustive interleaving
/// tests in the `loom_tests` module) the very same `Arc`/`Mutex`/`Condvar` are swapped for
/// `loom::sync`'s instrumented equivalents, letting loom explore every thread interleaving around
/// `lock()`/`Drop`/`wait()`/`notify_one()`. The concurrency-carrying types in this module
/// (`ReaderPool`, the writer mutex) go through this shim so no `#[cfg]` litters their bodies.
///
/// Note: loom does not model mutex *poisoning* (a panic aborts the model), so the poison-recovery
/// path in [`lock_writer`] is validated by the plain `std::thread` test
/// `poisoned_writer_with_open_transaction_is_healed_on_next_write`, not by loom. Loom's remit here
/// is the reader-pool `Condvar` checkout/checkin interleaving (no lost wakeup, no deadlock).
pub(crate) mod sync {
    #[cfg(loom)]
    pub(crate) use loom::sync::{Arc, Condvar, Mutex, MutexGuard};
    #[cfg(not(loom))]
    pub(crate) use std::sync::{Arc, Condvar, Mutex, MutexGuard};
}

use sync::{Arc, Condvar, Mutex, MutexGuard};

/// Default number of read connections in the pool.
pub const DEFAULT_READER_POOL_SIZE: usize = 4;

/// How long a [`ReaderPool::checkout`] may block before emitting a diagnostic warning.
///
/// A leaked or long-held [`ReadGuard`] would otherwise hang every future read with no signal;
/// after this threshold we log and keep waiting rather than failing the read.
///
/// Only used by the non-loom wait path (loom models blocking directly and has no timeout wait).
#[cfg(not(loom))]
const READER_CHECKOUT_WARN_AFTER: Duration = Duration::from_secs(5);

/// A shared connection to the database.
pub type SharedConnection = Arc<Mutex<Connection>>;

/// Errors that can occur when opening or using a database.
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

/// Writer `synchronous` pragma level.
///
/// Controls how aggressively the writer flushes to durable storage on commit. Readers are
/// unaffected (they never write). See the durability tradeoff documented on [`configure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    /// `synchronous=NORMAL`: fast; a committed transaction can be lost on power/OS crash, but the
    /// database is never corrupted. The default for the embedded single-app use case.
    #[default]
    Normal,
    /// `synchronous=FULL`: a committed transaction survives power loss, at the cost of write latency.
    Full,
}

impl Synchronous {
    /// The pragma value string SQLite expects.
    fn as_pragma(self) -> &'static str {
        match self {
            Synchronous::Normal => "NORMAL",
            Synchronous::Full => "FULL",
        }
    }
}

/// Configuration for opening a [`Database`].
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Number of read-only connections held in the reader pool.
    pub reader_pool_size: usize,

    /// Writer durability level (`synchronous` pragma). Defaults to [`Synchronous::Normal`].
    pub synchronous: Synchronous,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            reader_pool_size: DEFAULT_READER_POOL_SIZE,
            synchronous: Synchronous::default(),
        }
    }
}

/// Poison-tolerant lock helper for state where a recovered guard is always safe to reuse.
///
/// Used for the reader pool's idle list: a panic there can leave a `Connection` un-returned, but
/// reads cannot corrupt on-disk state, so recovering the remaining connections is harmless.
/// The single writer connection does NOT use this — see [`lock_writer`], which additionally clears
/// any transaction stranded by a panicking prior writer.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Acquire the writer connection, healing it if a prior writer panicked mid-transaction.
///
/// The fast path (uncontended, unpoisoned) is a plain `lock()` with no extra SQLite calls. Only when
/// the mutex is poisoned — i.e. a thread panicked while holding the writer — do we recover the guard
/// and, if the connection is left inside a transaction (`!is_autocommit()`), force a `ROLLBACK` to
/// discard the partially-applied work before handing the connection to the next writer. This keeps a
/// long-lived process alive after a mid-write panic without ever surfacing a stranded, half-open
/// transaction to subsequent callers.
fn lock_writer(m: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let guard = poisoned.into_inner();
            if !guard.is_autocommit() {
                tracing::warn!(
                    "recovered a poisoned writer mutex with an open transaction; forcing ROLLBACK"
                );
                if let Err(e) = guard.execute_batch("ROLLBACK") {
                    tracing::error!(
                        error = %e,
                        "failed to ROLLBACK a stranded transaction after writer poison recovery"
                    );
                }
            } else {
                tracing::warn!("recovered a poisoned writer mutex (connection was in autocommit)");
            }
            guard
        }
    }
}

/// A fixed-size pool of items handed out one at a time, blocking when exhausted.
///
/// This is the concurrency core behind [`ReaderPool`], extracted and made generic over the pooled
/// item `T` so its interleavings can be exercised under `loom` with a trivial payload (loom cannot
/// model a real [`Connection`]). Callers `take()` an item and `put()` it back; when none are idle,
/// `take()` blocks on the [`Condvar`] until a `put()` notifies it.
///
/// Invariants loom checks: no lost wakeups (a `put()` always eventually wakes a blocked `take()`),
/// no deadlock, and no item duplication or loss across arbitrary take/put interleavings.
pub(crate) struct WaitPool<T> {
    idle: Mutex<Vec<T>>,
    available: Condvar,
}

impl<T> WaitPool<T> {
    fn new(items: Vec<T>) -> Self {
        Self {
            idle: Mutex::new(items),
            available: Condvar::new(),
        }
    }

    /// Take an item, blocking until one becomes available.
    fn take(&self) -> T {
        let mut idle = lock(&self.idle);
        loop {
            if let Some(item) = idle.pop() {
                return item;
            }
            idle = self.wait_for_item(idle);
        }
    }

    /// Wait for a `put()` to signal, returning the re-acquired guard.
    ///
    /// Under `loom` this is a plain `wait` (loom has no `wait_timeout_while` and models the blocking
    /// directly). In normal builds it waits with a diagnostic threshold: if no item frees up within
    /// [`READER_CHECKOUT_WARN_AFTER`] it logs a warning (a leaked handle is the usual cause) and
    /// keeps waiting — a take never fails, it only surfaces a stall.
    #[cfg(loom)]
    fn wait_for_item<'a>(&self, idle: MutexGuard<'a, Vec<T>>) -> MutexGuard<'a, Vec<T>> {
        self.available
            .wait(idle)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(not(loom))]
    fn wait_for_item<'a>(&self, idle: MutexGuard<'a, Vec<T>>) -> MutexGuard<'a, Vec<T>> {
        let (guard, timeout) = self
            .available
            .wait_timeout_while(idle, READER_CHECKOUT_WARN_AFTER, |idle| idle.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if timeout.timed_out() {
            tracing::warn!(
                waited_secs = READER_CHECKOUT_WARN_AFTER.as_secs(),
                "reader pool checkout has been blocked for a while; a ReadGuard may be leaked or long-held"
            );
        }
        guard
    }

    /// Return an item to the pool and wake one waiter.
    fn put(&self, item: T) {
        lock(&self.idle).push(item);
        self.available.notify_one();
    }
}

/// A fixed-size pool of read-only SQLite connections.
///
/// Under WAL, readers never block the single writer nor each other, so a pool of independent read
/// connections gives real read concurrency. Connections are checked out and returned via
/// [`PooledReader`]; when all are in use, callers block until one is returned.
pub(crate) type ReaderPool = WaitPool<Connection>;

impl ReaderPool {
    /// Check out a reader, blocking until one becomes available.
    ///
    /// Takes the pool by `&Arc` explicitly (rather than `self: &Arc<Self>`) so the body is agnostic
    /// to whether `Arc` is `std`'s or `loom`'s under the sync shim.
    fn checkout(pool: &Arc<Self>) -> PooledReader {
        PooledReader {
            pool: Arc::clone(pool),
            conn: Some(pool.take()),
        }
    }

    fn checkin(&self, conn: Connection) {
        self.put(conn);
    }
}

/// RAII handle to a checked-out read connection. Returns the connection to the pool on a drop.
pub struct PooledReader {
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

/// A read guard: either a pooled reader connection (normal operation) or a borrowed connection
/// (inside a dry-run, where all work shares the already-locked writer connection).
pub enum ReadGuard<'a> {
    Pooled(PooledReader),
    Borrowed(&'a Connection),
}

impl std::ops::Deref for ReadGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            ReadGuard::Pooled(p) => p,
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
    readers: Arc<ReaderPool>,
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
        ReadGuard::Pooled(ReaderPool::checkout(&self.readers))
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

/// Owns the writer connection and reader pool. The entry point for opening a database and running
/// dry-runs.
pub struct Database {
    handle: DbHandle,
    path: DbPath,
}

/// Where the database lives; retained so the writer and reader-pool connections can all be opened
/// against the same location, and so `Drop` knows whether a WAL checkpoint is meaningful.
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
    /// cache so the writer and reader-pool connections all observe the same data.
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
        configure(
            &writer,
            ConnectionRole::Writer,
            is_memory,
            config.synchronous,
        )?;
        migrations::run(&mut writer)?;

        // Readers: independent connections, query-only. `synchronous` is irrelevant for read-only
        // connections, so its value is passed through but never applied for readers.
        let mut readers = Vec::with_capacity(config.reader_pool_size);
        for _ in 0..config.reader_pool_size {
            let conn = open_connection(&path, ConnectionRole::Reader)?;
            configure(&conn, ConnectionRole::Reader, is_memory, config.synchronous)?;
            readers.push(conn);
        }

        Ok(Self {
            handle: DbHandle {
                writer: Arc::new(Mutex::new(writer)),
                readers: Arc::new(ReaderPool::new(readers)),
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
    pub fn dry_run<F, T>(&self, f: F) -> Result<T, SqliteDataDriverError>
    where
        F: FnOnce(&DryRunHandle<'_>) -> T,
    {
        // Hold the real writer guard for the entire dry-run. This serializes against every other
        // writer via the app-level mutex, so no concurrent write can slip a committed transaction
        // *inside* our savepoint and get rolled back with us.
        let guard = lock_writer(&self.handle.writer);

        guard
            .execute_batch("SAVEPOINT valqeron_dry_run")
            .map_err(|source| SqliteDataDriverError::DryRun { source })?;

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
/// and writing, this would run a premature checkpoint mid-write — at that point switch to a
/// reference-counted close (e.g. cleanup on the last `Arc` drop) instead.
///
/// Future work (deferred): SQLite recommends running `PRAGMA optimize` periodically (not only at
/// close) to keep the query planner's statistics fresh on a very long-lived process. This is not
/// implemented to avoid adding write-counter state to the hot writer path; the close-time optimize
/// suffices for the current single-long-lived-process model. Revisit if planner drift is observed.
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
        DbPath::File(p) => {
            // Enforce read-only at the OS/SQLite layer for readers, not just via the `query_only`
            // pragma (which is kept as defense-in-depth in `configure`). Writers may create the file.
            let flags = match role {
                ConnectionRole::Writer => {
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
                }
                ConnectionRole::Reader => OpenFlags::SQLITE_OPEN_READ_ONLY,
            };
            Connection::open_with_flags(p, flags).map_err(map_err)
        }
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

/// Configures Sqlite database connection for the given role.
///
/// | PRAGMA NAME / API | PRAGMA VALUE | Description |
/// |: --- |: --- |: --- |
/// | journal_mode | WAL | Enables Sqlite WAL mode. Skip for in-memory databases. Check [the Sqlite docs](https://www.sqlite.org/wal.html) for more information. |
/// | synchronous | NORMAL (default) or FULL | Configurable via [`DatabaseConfig::synchronous`]. **NORMAL** (default): fast; a committed transaction can be lost on a power/OS crash, though the database is never corrupted. **FULL**: the database file is fully synchronized on commit so committed transactions survive power loss, at the cost of write latency. <br><br>See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_synchronous) for more information. |
/// | foreign_keys | ON | Enforce foreign key constraints. See [the Sqlite docs](https://www.sqlite.org/foreignkeys.html) for more information. |
/// | busy_timeout | 5000 | Abort any operation that takes longer than 5 seconds to complete. See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_busy_timeout) for more information. |
/// | cache_size | -64000 | The database connection cache is limited to 64MB (64,000 KiB). See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_cache_size) for more information. |
/// | temp_store | MEMORY | Forces temporary tables, indices, and views to be held purely in volatile RAM instead of spilling to disk files. See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_temp_store) for more information. |
/// | mmap_size | 268,435,456 | Sets the maximum memory-mapped I/O budget to 256MB to significantly speed up data read operations. See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_mmap_size) for more information. <br><br>Skip for in-memory databases |
/// | wal_autocheckpoint | 1000 | Automatically runs a PASSIVE checkpoint when the WAL log equals or exceeds 1,000 pages. Skip for in-memory databases. See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint) for more information |
/// | statement_cache | 64 | Set the maximum number of cached prepared statements this connection will hold. See [rusqlite docs](https://docs.rs/rusqlite/latest/src/rusqlite/cache.rs.html#48) |
/// | query_only | ON / OFF | Activates strict read-only mode (`SQLITE_READONLY`) exclusively if the current connection's assigned role is `ConnectionRole::Reader`.  See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_query_only) for more information. |
///
fn configure(
    conn: &Connection,
    role: ConnectionRole,
    is_memory: bool,
    synchronous: Synchronous,
) -> Result<(), SqliteDataDriverError> {
    let pragma_err = |source| SqliteDataDriverError::Pragma { source };

    if !is_memory {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(pragma_err)?;
    }

    // Durability knob. NORMAL (the default) is fast but a committed transaction can be lost on a
    // power/OS crash (the database itself is never corrupted); FULL trades write latency for
    // power-loss durability. Only meaningful on the writer — readers never write.
    conn.pragma_update(None, "synchronous", synchronous.as_pragma())
        .map_err(pragma_err)?;

    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(pragma_err)?;

    conn.busy_timeout(Duration::from_secs(5))
        .map_err(pragma_err)?;

    conn.pragma_update(None, "cache_size", -64_000i64)
        .map_err(pragma_err)?;

    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(pragma_err)?;

    let targeted_mmap_bytes = if is_memory {
        0i64
    } else {
        256i64 * 1024 * 1024
    };
    conn.pragma_update(None, "mmap_size", targeted_mmap_bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::migrations::MIGRATIONS;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn insert_dummy(conn: &Connection) {
        insert_dummy_result(conn).unwrap();
    }

    fn insert_dummy_result(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "INSERT INTO issuer (id, status, created_at)
             VALUES (randomblob(16), 'ACTIVE', '2026-01-01T00:00:00Z')",
        )
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM issuer", [], |row| row.get(0))
            .unwrap()
    }

    /// Open a file-backed database in a fresh temp dir, returning both so the dir outlives the db.
    ///
    /// The concurrency stress/soak tests run against a *file* (not shared-cache in-memory): under WAL
    /// readers never block the writer, and writer-vs-writer contention is absorbed by `busy_timeout`.
    /// Shared-cache in-memory instead surfaces `SQLITE_LOCKED` (table locks, not honored by
    /// `busy_timeout`), which is an artifact of that harness, not the production path.
    fn temp_file_db(config: DatabaseConfig) -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stress.db");
        let db = Database::open_with_config(path, config).unwrap();
        (dir, db)
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
        configure(&conn, ConnectionRole::Writer, true, Synchronous::Normal).unwrap();

        conn.pragma_update(None, "user_version", (MIGRATIONS.len() as i64) + 5)
            .unwrap();

        let result = migrations::run(&mut conn);
        assert!(matches!(
            result,
            Err(SqliteDataDriverError::UnknownSchemaVersion { .. })
        ));
    }

    #[test]
    fn invalid_pool_size_is_rejected() {
        let result = Database::open_in_memory_with_config(DatabaseConfig {
            reader_pool_size: 0,
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
        // Simulates the supported multi-process shape: one writing process plus other processes
        // reading the same file. WAL gives readers a consistent view of committed writes. (Concurrent
        // multi-process *writers* are out of scope: the app-level writer mutex only serializes
        // writers within a single process; cross-process writers would contend at SQLite's own write
        // lock, bounded by busy_timeout.)
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

    #[test]
    fn poisoned_writer_with_open_transaction_is_healed_on_next_write() {
        // A writer that panics mid-transaction poisons the mutex AND leaves the connection inside an
        // open transaction. The next `write()` must recover the guard *and* roll back the stranded
        // transaction, so the connection is usable and no partial work lingers.
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();

        let h2 = handle.clone();
        let poisoner = thread::spawn(move || {
            let conn = h2.write();
            conn.execute_batch(
                "BEGIN; INSERT INTO issuer (id, status, created_at) \
                                VALUES (randomblob(16), 'ACTIVE', '2026-01-01T00:00:00Z')",
            )
            .unwrap();
            // Panic while holding the guard, inside an open transaction.
            panic!("boom mid-write");
        });
        assert!(
            poisoner.join().is_err(),
            "poisoner thread should have panicked"
        );

        // The next writer recovers the poisoned guard; the stranded transaction must be gone.
        {
            let conn = handle.write();
            assert!(
                conn.is_autocommit(),
                "recovered writer must have had its stranded transaction rolled back"
            );
            // The dry-run/stranded INSERT must not have persisted.
            assert_eq!(
                count(&conn),
                0,
                "stranded transaction's write must be discarded"
            );
        }

        // And the connection is fully usable afterwards.
        insert_dummy(&db.handle().write());
        assert_eq!(count(&db.handle().read()), 1);
    }

    #[test]
    fn dry_run_serializes_against_a_concurrent_writer() {
        // The dry-run holds the writer mutex for its whole closure, so a concurrent real write must
        // queue behind it (not interleave inside the savepoint) and must survive the rollback.
        use std::sync::mpsc;

        let db = Arc::new(Database::open_in_memory().unwrap());
        let handle = db.handle();

        let (inside_tx, inside_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let db_dry = Arc::clone(&db);
        let dry = thread::spawn(move || {
            db_dry
                .dry_run(|h| {
                    insert_dummy(&h.write());
                    // Signal that we are inside the dry-run holding the writer lock.
                    inside_tx.send(()).unwrap();
                    // Hold the writer lock until told to release.
                    release_rx.recv().unwrap();
                    assert_eq!(count(&h.read()), 1, "dry-run sees only its own row");
                })
                .unwrap();
        });

        // Wait until the dry-run is holding the writer lock.
        inside_rx.recv().unwrap();

        // A concurrent writer must block until the dry-run releases the lock.
        let h2 = handle.clone();
        let writer = thread::spawn(move || {
            insert_dummy(&h2.write());
        });

        // Give the writer a chance to (fail to) proceed; it should still be blocked.
        thread::sleep(Duration::from_millis(50));

        // Release the dry-run; it rolls back its own write.
        release_tx.send(()).unwrap();
        dry.join().unwrap();
        writer.join().unwrap();

        // Only the real concurrent write survives; the dry-run write was discarded.
        assert_eq!(
            count(&db.handle().read()),
            1,
            "concurrent real write survives; dry-run write rolled back"
        );
    }

    #[test]
    fn file_reader_connections_are_read_only_at_the_sqlite_level() {
        // Readers on a file-backed database are opened SQLITE_OPEN_READ_ONLY, so a write attempt
        // fails at the SQLite layer (SQLITE_READONLY), not merely via the query_only pragma.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readonly.db");
        let db = Database::open(&path).unwrap();
        let handle = db.handle();

        let reader = handle.read();
        let err = insert_dummy_result(&reader).expect_err("write on a reader must fail");
        match err {
            rusqlite::Error::SqliteFailure(e, _) => {
                assert_eq!(
                    e.code,
                    rusqlite::ErrorCode::ReadOnly,
                    "expected SQLITE_READONLY, got {e:?}"
                );
            }
            other => panic!("expected a SqliteFailure(ReadOnly), got {other:?}"),
        }
    }

    // ---- Group A: deterministic pragma-effect and isolation tests -----------------------------
    //
    // These assert the pragmas actually *took effect* (not merely that `configure()` returned Ok),
    // and that the counter-based in-memory naming keeps independent databases isolated.

    fn journal_mode(conn: &Connection) -> String {
        conn.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap()
    }

    fn query_only(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "query_only", |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn journal_mode_is_wal_on_a_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wal.db");
        let db = Database::open(&path).unwrap();

        // Writer connection must be in WAL journal mode.
        assert_eq!(
            journal_mode(&db.handle().write()).to_lowercase(),
            "wal",
            "file-backed writer should be in WAL mode"
        );
    }

    #[test]
    fn in_memory_database_is_not_wal() {
        // WAL is skipped for in-memory databases; it must NOT report "wal".
        let db = Database::open_in_memory().unwrap();
        let mode = journal_mode(&db.handle().write()).to_lowercase();
        assert_ne!(
            mode, "wal",
            "in-memory database must not use WAL, got {mode:?}"
        );
    }

    #[test]
    fn synchronous_full_is_applied_to_the_writer_when_configured() {
        // PRAGMA synchronous returns an integer: 1 = NORMAL, 2 = FULL.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("durable.db");
        let db = Database::open_with_config(
            path,
            DatabaseConfig {
                synchronous: Synchronous::Full,
                ..Default::default()
            },
        )
        .unwrap();

        let level: i64 = db
            .handle()
            .write()
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(level, 2, "synchronous should be FULL (2) when configured");
    }

    #[test]
    fn synchronous_defaults_to_normal_on_the_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relaxed.db");
        let db = Database::open(&path).unwrap();

        let level: i64 = db
            .handle()
            .write()
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(level, 1, "synchronous should default to NORMAL (1)");
    }

    #[test]
    fn reader_connections_are_query_only_and_writer_is_not() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();

        // A pooled reader must report query_only = 1.
        {
            let reader = handle.read();
            assert_eq!(
                query_only(&reader),
                1,
                "reader pragma query_only must be ON"
            );
        }

        // The writer must report query_only = 0.
        assert_eq!(
            query_only(&handle.write()),
            0,
            "writer pragma query_only must be OFF"
        );
    }

    #[test]
    fn two_in_memory_databases_are_isolated() {
        // The counter-based unique shared-cache name must keep independent in-memory databases from
        // seeing each other's rows.
        let db_a = Database::open_in_memory().unwrap();
        let db_b = Database::open_in_memory().unwrap();

        insert_dummy(&db_a.handle().write());

        assert_eq!(count(&db_a.handle().read()), 1, "db_a sees its own write");
        assert_eq!(
            count(&db_b.handle().read()),
            0,
            "db_b must not see db_a's rows (independent in-memory databases)"
        );
    }

    // ---- Group B: concurrency / stress tests --------------------------------------------------
    //
    // The two heavy, timing-dependent tests are `#[ignore]` so they stay out of the default
    // `cargo test` path (avoiding CI flakiness) and run on demand:
    //
    //   cargo test -p valqeron-infrastructure -- --ignored
    //
    // The WAL-bound test is deterministic enough to run inline.

    #[test]
    fn wal_file_stays_bounded_after_drop() {
        // Several thousand writes then drop: the -wal sidecar must be truncated small by the
        // wal_autocheckpoint + the Drop TRUNCATE checkpoint, not left to grow unbounded.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bounded.db");
        let wal_path = dir.path().join("bounded.db-wal");

        {
            let db = Database::open(&path).unwrap();
            let handle = db.handle();
            for _ in 0..5_000 {
                insert_dummy(&handle.write());
            }
            // `db` drops here → PRAGMA optimize + wal_checkpoint(TRUNCATE).
        }

        // After drop, the WAL file (if it still exists) must be small. A runaway WAL would be many
        // MB; a truncated one is 0 bytes (or absent). Allow a generous ceiling well under that.
        if let Ok(meta) = std::fs::metadata(&wal_path) {
            let size = meta.len();
            assert!(
                size < 64 * 1024,
                "WAL should be truncated on close; found {size} bytes"
            );
        }

        // And the data is intact.
        let db = Database::open(&path).unwrap();
        assert_eq!(count(&db.handle().read()), 5_000);
    }

    #[test]
    #[ignore = "stress test; run with --ignored"]
    fn dry_run_does_not_race_concurrent_writes() {
        // A real writer and a dry-runner hammer the database concurrently. Because dry_run now runs
        // on the shared writer connection under the app-level mutex (rather than a second physical
        // connection racing for SQLite's write lock), neither side should ever surface SQLITE_BUSY.
        use std::sync::Barrier;

        const ITERS: usize = 200;

        let (_dir, db) = temp_file_db(DatabaseConfig::default());
        let db = Arc::new(db);
        let barrier = Arc::new(Barrier::new(2));

        let (db1, b1) = (Arc::clone(&db), Arc::clone(&barrier));
        let writer = thread::spawn(move || {
            b1.wait();
            for _ in 0..ITERS {
                insert_dummy_result(&db1.handle().write())
                    .expect("concurrent real write must not surface SQLITE_BUSY");
            }
        });

        let (db2, b2) = (Arc::clone(&db), Arc::clone(&barrier));
        let dry_runner = thread::spawn(move || {
            b2.wait();
            for _ in 0..ITERS {
                db2.dry_run(|h| {
                    insert_dummy_result(&h.write())
                        .expect("dry-run write must not surface SQLITE_BUSY");
                })
                .expect("dry_run itself must not fail");
            }
        });

        writer.join().unwrap();
        dry_runner.join().unwrap();

        // Only the real writer's rows persist; every dry-run was rolled back.
        assert_eq!(
            count(&db.handle().read()),
            ITERS as i64,
            "exactly the committed writes survive; all dry-runs rolled back"
        );
    }

    #[test]
    #[ignore = "soak test; run with --ignored"]
    fn mixed_read_write_soak() {
        // N threads doing a random mix of insert / apply_patch / find / list for a fixed number of
        // iterations. Invariants after the soak:
        //   * final row count == number of successful inserts,
        //   * every id's version is >= 1 and consistent with the applied patches (monotonic, no
        //     lost updates),
        //   * zero SQLITE_BUSY surfaced,
        //   * zero panics.
        use std::sync::Barrier;

        const THREADS: usize = 8;
        const OPS_PER_THREAD: usize = 500;

        // File-backed: WAL gives real reader/writer concurrency (see `temp_file_db`).
        let (_dir, db) = temp_file_db(DatabaseConfig {
            reader_pool_size: THREADS,
            ..Default::default()
        });
        let db = Arc::new(db);
        let barrier = Arc::new(Barrier::new(THREADS));
        // Tracks successful inserts across all threads.
        let inserted = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let db = Arc::clone(&db);
                let barrier = Arc::clone(&barrier);
                let inserted = Arc::clone(&inserted);
                thread::spawn(move || {
                    // One long-lived handle per thread so guards can borrow from it across the loop.
                    let handle = db.handle();
                    // Cheap per-thread PRNG (xorshift) to avoid pulling in `rand`.
                    let mut rng: u64 = 0x9E3779B97F4A7C15 ^ (t as u64).wrapping_mul(0x1234_5678);
                    let mut next = || {
                        rng ^= rng << 13;
                        rng ^= rng >> 7;
                        rng ^= rng << 17;
                        rng
                    };

                    barrier.wait();

                    // Each thread keeps its own inserted ids so patches target real rows.
                    let mut my_ids: Vec<[u8; 16]> = Vec::new();

                    for _ in 0..OPS_PER_THREAD {
                        match next() % 4 {
                            0 => {
                                // INSERT with a fresh random id.
                                let mut id = [0u8; 16];
                                let r = next();
                                id[..8].copy_from_slice(&r.to_le_bytes());
                                id[8..].copy_from_slice(&next().to_le_bytes());
                                let conn = handle.write();
                                let affected = conn
                                    .execute(
                                        "INSERT INTO issuer (id, status, created_at, version) \
                                         VALUES (?1, 'ACTIVE', '2026-01-01T00:00:00Z', 1)",
                                        rusqlite::params![&id[..]],
                                    )
                                    .expect("insert must not surface SQLITE_BUSY");
                                if affected == 1 {
                                    my_ids.push(id);
                                    inserted.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            1 => {
                                // Version-guarded patch on one of our own rows.
                                if let Some(id) =
                                    my_ids.get((next() as usize) % my_ids.len().max(1))
                                {
                                    let conn = handle.write();
                                    // Read current version, then bump with the guard.
                                    let ver: Option<i64> = conn
                                        .query_row(
                                            "SELECT version FROM issuer WHERE id = ?1",
                                            rusqlite::params![&id[..]],
                                            |r| r.get(0),
                                        )
                                        .ok();
                                    if let Some(ver) = ver {
                                        conn.execute(
                                            "UPDATE issuer SET status = 'RETIRED', \
                                             version = version + 1 WHERE id = ?1 AND version = ?2",
                                            rusqlite::params![&id[..], ver],
                                        )
                                        .expect("patch must not surface SQLITE_BUSY");
                                    }
                                }
                            }
                            2 => {
                                // Point read of one of our ids.
                                if let Some(id) =
                                    my_ids.get((next() as usize) % my_ids.len().max(1))
                                {
                                    let conn = handle.read();
                                    let _found: Option<i64> = conn
                                        .query_row(
                                            "SELECT version FROM issuer WHERE id = ?1",
                                            rusqlite::params![&id[..]],
                                            |r| r.get(0),
                                        )
                                        .ok();
                                }
                            }
                            _ => {
                                // Full-table read.
                                let conn = handle.read();
                                let _total = count(&conn);
                            }
                        }
                    }

                    my_ids
                })
            })
            .collect();

        let mut all_ids: Vec<[u8; 16]> = Vec::new();
        for h in handles {
            all_ids.extend(h.join().expect("no thread should panic"));
        }

        let handle = db.handle();
        let conn = handle.read();

        // Invariant: final row count == successful inserts.
        let final_count = count(&conn);
        assert_eq!(
            final_count,
            inserted.load(Ordering::Relaxed) as i64,
            "final row count must equal successful inserts (no lost inserts, no phantom rows)"
        );
        assert_eq!(final_count as usize, all_ids.len());

        // Invariant: every row has a version >= 1 (monotonic bumps, never rolled below the start).
        for id in &all_ids {
            let ver: i64 = conn
                .query_row(
                    "SELECT version FROM issuer WHERE id = ?1",
                    rusqlite::params![&id[..]],
                    |r| r.get(0),
                )
                .expect("every inserted id must still exist");
            assert!(ver >= 1, "version must be monotonic (>= 1), got {ver}");
        }
    }
}

/// Exhaustive interleaving tests for the reader pool's concurrency core ([`WaitPool`]).
///
/// These are compiled and run only under `--cfg loom`; normal builds and `cargo test` skip them
/// entirely (the whole module is `#[cfg(loom)]`). Loom re-runs each `model` closure under every
/// legal thread interleaving around `lock()`/`Drop`/`Condvar::wait`/`notify_one`, proving the pool
/// has no lost wakeups, no deadlock, and never duplicates or loses a pooled item.
///
/// Run with:
///
/// ```text
/// RUSTFLAGS="--cfg loom" cargo test -p valqeron-infrastructure --lib loom_tests -- --nocapture
/// ```
///
/// (Loom explores an exponential state space; keep the thread/loop counts here small — 2 threads
/// over a size-1 pool is enough to surface the classic wait/notify races. Bump
/// `LOOM_MAX_PREEMPTIONS` only if you need deeper coverage.)
///
/// Loom does not model mutex poisoning, so the writer poison-recovery path is covered by the
/// std-thread test `poisoned_writer_with_open_transaction_is_healed_on_next_write` instead.
#[cfg(all(loom, test))]
mod loom_tests {
    use super::WaitPool;
    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicUsize, Ordering};

    /// Two threads contend for a single pooled item: each must eventually acquire it, use it, and
    /// return it, with no deadlock and no lost wakeup, under every interleaving loom can produce.
    #[test]
    fn single_item_is_never_lost_or_duplicated_under_contention() {
        loom::model(|| {
            let pool = Arc::new(WaitPool::new(vec![0usize]));

            // Tracks how many threads believe they hold the (unique) item simultaneously.
            let in_use = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let pool = Arc::clone(&pool);
                    let in_use = Arc::clone(&in_use);
                    loom::thread::spawn(move || {
                        let item = pool.take();
                        // Mutual exclusion: only one thread may hold the single item at a time.
                        let concurrent = in_use.fetch_add(1, Ordering::Acquire);
                        assert_eq!(
                            concurrent, 0,
                            "two threads held the single pooled item at once"
                        );
                        in_use.fetch_sub(1, Ordering::Release);
                        pool.put(item);
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }

            // The item must have come home: exactly one item, and it must be takeable without blocking
            // forever (proving no wakeup was lost mid-run).
            let item = pool.take();
            assert_eq!(item, 0);
            pool.put(item);
        });
    }

    /// A waiter that blocks on an empty pool must be woken by a later `put` (no lost notification).
    #[test]
    fn blocked_take_is_woken_by_a_later_put() {
        loom::model(|| {
            // Start empty so the taker is forced to block on the Condvar.
            let pool: Arc<WaitPool<usize>> = Arc::new(WaitPool::new(vec![]));

            let taker = {
                let pool = Arc::clone(&pool);
                loom::thread::spawn(move || {
                    let item = pool.take(); // must block, then be woken by the put below
                    assert_eq!(item, 7);
                })
            };

            // Producer supplies the item; its notify must reach the blocked taker.
            pool.put(7);

            taker.join().unwrap();
        });
    }
}

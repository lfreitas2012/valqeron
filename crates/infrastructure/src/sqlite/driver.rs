use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

use crate::sqlite::db::PooledReader;
use sync::{Arc, Condvar, Mutex, MutexGuard};

/// Default number of read connections in the pool.
pub const DEFAULT_READER_POOL_SIZE: usize = 4;

/// How long a [`ReaderPool::checkout`] may block before emitting a diagnostic warning.
///
/// A leaked or long-held [`ReadGuard`] would otherwise hang every future read with no signal;
/// after this threshold we log and keep waiting rather than failing the read.
///
/// Only used by the non-loom wait path (loom models blocking directly and have no timeout wait).
#[cfg(not(loom))]
const READER_CHECKOUT_WARN_AFTER: Duration = Duration::from_secs(5);

/// Maximum duration a single SQL query is allowed to run before being aborted.
const MAX_QUERY_EXECUTION_TIME: Duration = Duration::from_secs(15);

/// Minimum idle gap between progress checks to detect a new query invocation.
const IDLE_RESET_THRESHOLD: Duration = Duration::from_millis(50);

/// A shared connection to the database.
pub type SharedConnection = Arc<Mutex<Connection>>;

/// Writer `synchronous` pragma level.
///
/// Controls how aggressively the writer flushes to durable storage on commit. Readers are unaffected
/// (they never write). See the durability tradeoff documented on [`crate::sqlite::driver::configure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    /// `synchronous=NORMAL`.
    ///
    /// Fast. A committed transaction can be lost in a power/OS crash, but the database is never corrupted.
    #[default]
    Normal,

    /// `synchronous=FULL`:
    ///
    /// Slower. A committed transaction survives power loss.
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

/// Poison-tolerant lock helper for a state where a recovered guard is always safe to reuse.
///
/// Used for the reader pool's idle list: a panic there can leave a `Connection` unreturned, but
/// reads cannot corrupt on-disk state, so recovering the remaining connections is harmless. The
/// single writer connection does NOT use this; see [`lock_writer`], which additionally clears any
/// transaction stranded by a panicking prior writer.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Acquire the writer connection, healing it if a prior writer panicked mid-transaction.
///
/// The fast path (uncontended, unpoisoned) is a plain `lock()` with no extra SQLite calls. Only
/// when the mutex is poisoned (i.e., a thread panicked while holding the writer) do we recover the
/// guard and, if the connection is left inside a transaction (`!is_autocommit()`), force a
/// `ROLLBACK` to discard the partially applied work before handing the connection to the next writer.
/// This keeps a long-lived process alive after a mid-write panic without ever surfacing a stranded,
/// half-open transaction to later callers.
pub(crate) fn lock_writer(m: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    m.lock().unwrap_or_else(|poisoned| {
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
    })
}

/// A fixed-size pool of items handed out one at a time, blocking when exhausted.
///
/// This is the concurrency core behind [`ReaderPool`], extracted and made generic over the pooled
/// item `T` so its interleaving can be exercised under `loom` with a trivial payload (loom cannot
/// model a real [`Connection`]). Callers `take()` an item and `put()` it back; when none are idle,
/// `take()` blocks on the [`Condvar`] until a `put()` notifies it.
///
/// Invariants' loom checks: no lost wakeup (a `put()` always eventually wakes a blocked `take()`),
/// no deadlock, and no item duplication or loss across arbitrary take/put interleaving.
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
    /// keeps waiting; A take never fails, it only surfaces a stall.
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
    pub(crate) fn checkout(pool: &Arc<Self>) -> PooledReader {
        PooledReader {
            pool: Arc::clone(pool),
            conn: Some(pool.take()),
        }
    }

    pub(crate) fn checkin(&self, conn: Connection) {
        self.put(conn);
    }
}

/// Where a [`DbHandle`] gets its read connections from.
#[derive(Clone)]
pub(crate) enum ReaderSource {
    /// Independent read-only connections (file-backed, WAL). Real read concurrency.
    Pool(Arc<ReaderPool>),

    /// No separate reader connections. Reads take the writer mutex, same as writes.
    ///
    /// Used for in-memory `Database`s. SQLite cannot share an in-memory database's content across
    /// independently opened connections without a shared-cache mode. There is no `vfs=memdb`
    /// escape hatch for this; sharing requires `cache=shared` regardless of VFS. Rather than take
    /// on shared-cache's separate table-level locking model for the in-memory/test-only path,
    /// in-memory `Database`s simply don't open extra physical connections: this is fine, since
    /// `open_in_memory` exists for unit tests, not for the read-concurrency guarantees the
    /// file+WAL production path provides.
    SharedWithWriter,
}

/// Where the database lives; retained so the writer and reader-pool connections can all be opened
/// against the same location, and so `Drop` knows whether a WAL checkpoint is meaningful.
#[derive(Clone)]
pub(crate) enum DbPath {
    File(PathBuf),
    /// A named in-memory database served by SQLite's `memdb` VFS (available since SQLite 3.36.0).
    /// Multiple connections opened against the same name see the same content for the lifetime of
    /// at least one open handle — without opting the process into shared-cache mode
    /// (https://www.sqlite.org/c3ref/enable_shared_cache.html) and its table-level locking quirks.
    Memory(String),
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
        DbPath::Memory(_name) => {
            let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
            Connection::open_in_memory_with_flags(flags).map_err(map_err)
        }
    }
}

/// Configures Sqlite database connection for the given role.
///
/// | PRAGMA NAME / API | PRAGMA VALUE | Description |
/// |: --- |: --- |: --- |
/// | journal_mode | WAL | Enables Sqlite WAL mode. Skip for in-memory databases. Check [Sqlite docs](https://www.sqlite.org/wal.html) for more information. |
/// | synchronous | NORMAL (default) or FULL | Configurable via [`DatabaseConfig::synchronous`]. **NORMAL** (default): fast; a committed transaction can be lost in a power/OS crash, though the database is never corrupted. **FULL**: the database file is fully synchronized on commit so committed transactions survive power loss, at the cost of write latency. <br><br>See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_synchronous) for more information. |
/// | foreign_keys | ON | Enforce foreign key constraints. See [Sqlite docs](https://www.sqlite.org/foreignkeys.html) for more information. |
/// | busy_timeout | 5000 | Abort any operation that takes longer than 5 seconds to complete. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_busy_timeout) for more information. |
/// | cache_size | -64000 | The database connection cache is limited to 64MB (64,000 KiB). See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_cache_size) for more information. |
/// | temp_store | MEMORY | Forces temporary tables, indices, and views to be held purely in volatile RAM instead of spilling to disk files. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_temp_store) for more information. |
/// | mmap_size | 268,435,456 | Sets the maximum memory-mapped I/O budget to 256MB to significantly speed up data read operations. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_mmap_size) for more information. <br><br>Skip for in-memory databases |
/// | wal_autocheckpoint | 1000 | Automatically runs a PASSIVE checkpoint when the WAL log equals or exceeds 1,000 pages. Skip for in-memory databases. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint) for more information |
/// | statement_cache | 64 | Set the maximum number of cached prepared statements this connection will hold. See [rusqlite docs](https://docs.rs/rusqlite/latest/src/rusqlite/cache.rs.html#48) |
/// | query_only | ON / OFF | Activates strict read-only mode (`SQLITE_READONLY`) exclusively if the current connection's assigned role is `ConnectionRole::Reader`.  See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_query_only) for more information. |
///
fn configure(
    conn: &Connection,
    role: ConnectionRole,
    is_memory: bool,
    synchronous: Synchronous,
) -> Result<(), SqliteDataDriverError> {
    debug_assert!(
        !(is_memory && matches!(role, ConnectionRole::Reader)),
        "in-memory databases have no reader pool — reads share the writer connection \
         (see ReaderSource::SharedWithWriter), so this combination should never occur"
    );

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

    configure_sqlite_progress_handler(conn);

    Ok(())
}

/// Configures a progress handler on `conn` to enforce a wall-clock timeout on individual SQL queries.
///
/// Registers a callback invoked every 5,000 SQLite VM instructions:
/// - Resets the statement timer if elapsed time since the last callback exceeds [`IDLE_RESET_THRESHOLD`],
///   indicating the connection was idle between pool uses.
/// - Aborts the current query with `SQLITE_INTERRUPT` if execution exceeds [`MAX_QUERY_EXECUTION_TIME`].
fn configure_sqlite_progress_handler(conn: &Connection) {
    let mut query_start = Instant::now();
    let mut last_check = Instant::now();

    let _ = conn.progress_handler(
        5_000,
        Some(move || {
            let now = Instant::now();

            // Reset query_start if the connection was idle in the pool
            if now.duration_since(last_check) > IDLE_RESET_THRESHOLD {
                query_start = now;
            }
            last_check = now;

            if now.duration_since(query_start) > MAX_QUERY_EXECUTION_TIME {
                tracing::error!(
                    "SQLite query interrupted: execution exceeded time limit of {:?}",
                    MAX_QUERY_EXECUTION_TIME
                );
                true // Returns SQLITE_INTERRUPT to rusqlite
            } else {
                false
            }
        }),
    );
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
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_with_config(
            dir.path().join("test.db"),
            DatabaseConfig {
                reader_pool_size: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let handle = db.handle();
        let write_guard = handle.write();
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
    fn in_memory_reads_share_the_writer_connection_and_are_not_query_only() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();
        assert_eq!(
            query_only(&handle.read()),
            0,
            "in-memory reads share the writer connection and are never query_only"
        );
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
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
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

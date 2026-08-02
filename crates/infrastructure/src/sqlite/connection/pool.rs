//! The reader pool and writer-lock helpers.
//!
//! Under WAL, readers never block the single writer nor each other, so a fixed pool of independent
//! read connections gives real read concurrency. This module owns that pool ([`ReaderPool`], built
//! on the loom-testable generic [`WaitPool`]), the RAII checkout handle ([`PooledReader`]), the
//! reader-source selector ([`ReaderSource`]), and the poison-tolerant lock helpers ([`lock`],
//! [`lock_writer`]).

use rusqlite::Connection;

use crate::sqlite::connection::sync::{Arc, Condvar, Mutex, MutexGuard};

/// How long a [`ReaderPool::checkout`] may block before emitting a diagnostic warning.
///
/// A leaked or long-held read guard would otherwise hang every future read with no signal; after
/// this threshold we log and keep waiting rather than failing the read.
///
/// Only used by the non-loom wait path (loom models blocking directly and has no timeout wait).
#[cfg(not(loom))]
const READER_CHECKOUT_WARN_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

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
    pub(crate) fn new(items: Vec<T>) -> Self {
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

/// RAII handle to a checked-out read connection. Returns the connection to the pool on a drop.
pub(crate) struct PooledReader {
    pool: Arc<ReaderPool>,
    conn: Option<Connection>,
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

/// Where a [`DbHandle`](crate::sqlite::connection::DbHandle) gets its read connections from.
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

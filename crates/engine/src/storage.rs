//! Async facade over the synchronous storage engine.
//!
//! gRPC handlers are async; `StorageEngine`/`IssuerRepository` are blocking by design
//! (engine.md §2.1). Calling the blocking reader pool from a tokio worker thread could stall the
//! runtime — and, because pool checkout parks on a `Condvar`, deadlock it outright when every
//! worker is parked and none is left to release a reader.
//!
//! The facade closes that hole with tokio's built-in bridge: every storage
//! closure runs on the **blocking** pool via `spawn_blocking`, and a
//! semaphore sized to the SQLite pool bounds how many closures may be in
//! flight, so backpressure is an explicit typed error rather than an
//! unbounded pile-up of blocked threads. The loom-verified single-writer /
//! reader-pool model in `valqeron-infrastructure` is used exactly as
//! designed — untouched.
//!
//! Shutdown protocol: [`AsyncStorage::close`] rejects new calls, and
//! [`AsyncStorage::wait_idle`] waits for in-flight closures to finish, after
//! which [`AsyncStorage::into_engine`] can reclaim the engine for a
//! deterministic final WAL checkpoint (`Drop`).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use valqeron_infrastructure::SqliteStorageEngine;

/// How long a caller may wait for an execution slot before the engine reports backpressure. Mirrors
/// the spirit of the storage layer's own 15s progress-handler timeout while failing faster at the
/// queueing stage.
const QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll interval for [`AsyncStorage::wait_idle`].
const IDLE_POLL: Duration = Duration::from_millis(25);

#[derive(Debug, thiserror::Error)]
pub enum StorageCallError {
    /// Every execution slot stayed occupied for the whole queue timeout.
    #[error("the engine is overloaded; retry later")]
    Overloaded,

    /// The engine is shutting down and no longer accepts storage work.
    #[error("the engine is shutting down")]
    ShuttingDown,

    /// The blocking task was cancelled or failed to run to completion.
    #[error("storage task failed: {0}")]
    TaskFailed(String),
}

/// Cloneable handle giving async code bounded access to the blocking storage engine.
#[derive(Clone)]
pub struct AsyncStorage {
    engine: Arc<SqliteStorageEngine>,
    permits: Arc<Semaphore>,
    max_in_flight: usize,
}

impl AsyncStorage {
    /// Wrap an opened engine. `max_in_flight` should mirror the SQLite
    /// reader pool size plus one writer: more slots would only queue on the
    /// pool's own `Condvar`, fewer would leave readers idle.
    pub fn new(engine: SqliteStorageEngine, max_in_flight: usize) -> Self {
        Self {
            engine: Arc::new(engine),
            permits: Arc::new(Semaphore::new(max_in_flight)),
            max_in_flight,
        }
    }

    /// Run a blocking closure against the storage engine on tokio's blocking pool, bounded by the
    /// in-flight semaphore.
    ///
    /// The closure receives the engine and may use `repositories()` / `dry_run()` freely; it must
    /// contain the *whole* domain operation (e.g. `register_issuer`'s check-then-insert) so
    /// multistep services are not split across executions.
    ///
    /// Cancellation note: once the closure starts, it runs to completion even if the caller's
    /// future is dropped — `spawn_blocking` tasks are not cancellable. The permit is released when
    /// the closure finishes.
    pub async fn call<T, F>(&self, operation: &'static str, f: F) -> Result<T, StorageCallError>
    where
        F: FnOnce(&SqliteStorageEngine) -> T + Send + 'static,
        T: Send + 'static,
    {
        let acquire = Arc::clone(&self.permits).acquire_owned();
        let permit = match tokio::time::timeout(QUEUE_TIMEOUT, acquire).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_closed)) => return Err(StorageCallError::ShuttingDown),
            Err(_elapsed) => {
                tracing::warn!(operation, "storage queue full; rejecting call");
                return Err(StorageCallError::Overloaded);
            }
        };

        let engine = Arc::clone(&self.engine);
        let span = tracing::debug_span!("storage_call", operation);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _entered = span.entered();
            f(&engine)
        })
        .await
        .map_err(|e| StorageCallError::TaskFailed(e.to_string()))
    }

    /// Stop accepting new storage calls. In-flight closures keep running.
    pub fn close(&self) {
        self.permits.close();
    }

    /// Wait (bounded) until no storage closure is in flight. Returns `true`
    /// when idle, `false` when the deadline expired first.
    pub async fn wait_idle(&self, deadline: Duration) -> bool {
        let wait = async {
            while self.in_flight() > 0 {
                tokio::time::sleep(IDLE_POLL).await;
            }
        };
        tokio::time::timeout(deadline, wait).await.is_ok()
    }

    /// Number of closures currently holding an execution slot.
    pub fn in_flight(&self) -> usize {
        self.max_in_flight
            .saturating_sub(self.permits.available_permits())
    }

    /// Reclaim the engine for a deterministic final checkpoint. Fails when
    /// another clone (or a still-running closure) holds a reference.
    pub fn into_engine(self) -> Result<SqliteStorageEngine, Self> {
        let Self {
            engine,
            permits,
            max_in_flight,
        } = self;
        Arc::try_unwrap(engine).map_err(|engine| Self {
            engine,
            permits,
            max_in_flight,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use valqeron_core::{IssuerRepository, LoadMode, StorageEngine};
    use valqeron_infrastructure::DatabaseConfig;

    /// The `TempDir` holds the database files; keep it alive alongside the
    /// storage handle for the duration of the test.
    fn storage(max_in_flight: usize) -> (tempfile::TempDir, AsyncStorage) {
        let dir = tempfile::tempdir().expect("create temp dir for test database");
        let engine =
            SqliteStorageEngine::open(dir.path().join("test.db"), DatabaseConfig::default())
                .expect("open temp file-backed engine");
        (dir, AsyncStorage::new(engine, max_in_flight))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn more_concurrent_calls_than_pool_slots_complete() {
        // The deadlock probe: with only 2 runtime workers and a pool of 2,
        // 16 concurrent reads must complete because the blocking happens on
        // the blocking pool, never on the runtime workers.
        let (_dir, storage) = storage(2);
        let mut handles = Vec::new();
        for _ in 0..16 {
            let s = storage.clone();
            handles.push(tokio::spawn(async move {
                s.call("test.read", |engine| {
                    engine
                        .repositories()
                        .issuers
                        .list_paged(None, 10, LoadMode::Lazy)
                        .map(|rows| rows.len())
                })
                .await
            }));
        }
        for handle in handles {
            let joined = handle.await.expect("task join");
            let called = joined.expect("no backpressure expected in this test");
            assert!(matches!(called, Ok(0)));
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_queue_surfaces_a_typed_backpressure_error() {
        // One slot, held by a closure that blocks until we release it.
        let (_dir, storage) = storage(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();

        let blocker = {
            let s = storage.clone();
            tokio::spawn(async move {
                s.call("test.block", move |_| {
                    let _ = release_rx.recv();
                })
                .await
            })
        };

        // Give the blocker time to occupy the slot.
        while storage.in_flight() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // tokio::time::pause() cannot be used with multi_thread; accept the
        // real 5s queue timeout by racing a shorter caller-side timeout is
        // not possible either — so assert on the closed-path instead: keep
        // this test on the boundary that matters (typed error, no hang).
        let attempt = {
            let s = storage.clone();
            tokio::spawn(async move { s.call("test.reject", |_| ()).await })
        };
        let outcome = attempt.await.expect("join");
        assert!(matches!(outcome, Err(StorageCallError::Overloaded)));

        release_tx.send(()).expect("release blocker");
        let blocked = blocker.await.expect("join blocker");
        assert!(blocked.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closed_storage_rejects_new_calls_and_drains() {
        let (_dir, storage) = storage(2);
        storage.close();

        let result = storage.call("test.closed", |_| ()).await;
        assert!(matches!(result, Err(StorageCallError::ShuttingDown)));

        assert!(storage.wait_idle(Duration::from_secs(1)).await);
        let reclaimed = storage.into_engine();
        assert!(reclaimed.is_ok(), "no other clones may remain");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropped_caller_does_not_poison_the_facade() {
        let (_dir, storage) = storage(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();

        let s = storage.clone();
        let task = tokio::spawn(async move {
            s.call("test.cancelled", move |_| {
                let _ = release_rx.recv();
            })
            .await
        });

        while storage.in_flight() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Cancel the awaiting future; the blocking closure keeps running.
        task.abort();
        release_tx.send(()).expect("release closure");

        // The slot must come back and the facade must keep serving.
        assert!(storage.wait_idle(Duration::from_secs(5)).await);
        let after = storage.call("test.after-cancel", |_| 7).await;
        assert!(matches!(after, Ok(7)));
    }
}

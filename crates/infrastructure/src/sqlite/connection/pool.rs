use rusqlite::Connection;

use crate::sqlite::connection::sync::{Arc, Condvar, Mutex, MutexGuard};

#[cfg(not(loom))]
const READER_CHECKOUT_WARN_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

    fn take(&self) -> T {
        let mut idle = lock(&self.idle);
        loop {
            if let Some(item) = idle.pop() {
                return item;
            }
            idle = self.wait_for_item(idle);
        }
    }

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

    fn put(&self, item: T) {
        lock(&self.idle).push(item);
        self.available.notify_one();
    }
}

pub(crate) type ReaderPool = WaitPool<Connection>;

impl ReaderPool {
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

#[derive(Clone)]
pub(crate) enum ReaderSource {
    Pool(Arc<ReaderPool>),

    SharedWithWriter,
}

#[cfg(all(loom, test))]
mod loom_tests {
    use super::WaitPool;
    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn single_item_is_never_lost_or_duplicated_under_contention() {
        loom::model(|| {
            let pool = Arc::new(WaitPool::new(vec![0usize]));

            let in_use = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let pool = Arc::clone(&pool);
                    let in_use = Arc::clone(&in_use);
                    loom::thread::spawn(move || {
                        let item = pool.take();
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

            let item = pool.take();
            assert_eq!(item, 0);
            pool.put(item);
        });
    }

    #[test]
    fn blocked_take_is_woken_by_a_later_put() {
        loom::model(|| {
            let pool: Arc<WaitPool<usize>> = Arc::new(WaitPool::new(vec![]));

            let taker = {
                let pool = Arc::clone(&pool);
                loom::thread::spawn(move || {
                    let item = pool.take();
                    assert_eq!(item, 7);
                })
            };

            pool.put(7);

            taker.join().unwrap();
        });
    }
}

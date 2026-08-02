//! Thread-local pinning of the connection a dry-run runs on.
//!
//! [`Database::dry_run`](crate::sqlite::connection::Database::dry_run) holds the writer lock for the
//! whole closure and publishes the already-locked `&Connection` here for its duration. A
//! [`DbHandle::DryRun`](crate::sqlite::connection::DbHandle) reads it so every repository read/write
//! inside the closure runs on that single connection — inside the dry-run `SAVEPOINT` — without
//! re-locking the writer mutex (which would self-deadlock).

use std::cell::Cell;

use rusqlite::Connection;

thread_local! {
    /// The connection a dry-run is currently pinned to on this thread, if any.
    ///
    /// The pointer is only ever dereferenced on the same thread while the guard that produced it is
    /// alive, so the borrow is sound.
    static DRY_RUN_CONN: Cell<Option<*const Connection>> = const { Cell::new(None) };
}

/// Publish `conn` as the active dry-run connection for the current thread for the duration of `f`,
/// restoring the previous value afterwards (supporting nested dry-runs).
pub(crate) fn with_dry_run_conn<T>(conn: &Connection, f: impl FnOnce() -> T) -> T {
    let previous = DRY_RUN_CONN.with(|slot| slot.replace(Some(conn as *const Connection)));
    let result = f();
    DRY_RUN_CONN.with(|slot| slot.set(previous));
    result
}

/// Fetch the current thread's pinned dry-run connection.
///
/// # Panics
///
/// Panics if called outside an active dry-run (a `DbHandle::DryRun` must only exist inside
/// [`with_dry_run_conn`]).
///
/// # Safety
///
/// The returned reference is valid only because
/// [`Database::dry_run`](crate::sqlite::connection::Database::dry_run) keeps the underlying locked
/// connection alive for the whole closure on this thread and clears the slot before returning.
pub(crate) fn current_dry_run_conn() -> &'static Connection {
    let ptr = DRY_RUN_CONN
        .with(|slot| slot.get())
        .expect("DbHandle::DryRun used outside an active dry-run");
    // SAFETY: the pointer was published by `with_dry_run_conn` from a live, locked `&Connection`
    // that outlives every access on this thread; the slot is cleared before that connection is
    // released, so a dangling pointer can never be observed here.
    unsafe { &*ptr }
}

//! Thread-local pinning for dry-run connections.
//!
//! [`Database::dry_run`](crate::sqlite::connection::Database::dry_run) publishes its locked
//! connection here for the closure's duration. [`DbHandle::DryRun`](crate::sqlite::connection::DbHandle)
//! uses it for all repository operations without locking the writer again.

use std::cell::Cell;

use rusqlite::Connection;

thread_local! {
    /// The connection currently pinned for a dry-run on this thread, if any.
    static DRY_RUN_CONN: Cell<Option<*const Connection>> = const { Cell::new(None) };
}

/// Pins `conn` for `f` on the current thread, then restores the previous value.
pub(crate) fn with_dry_run_conn<T>(conn: &Connection, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<*const Connection>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DRY_RUN_CONN.set(self.0);
        }
    }

    let _restore = Restore(DRY_RUN_CONN.replace(Some(conn as *const Connection)));
    f()
}

/// Whether a dry-run connection is pinned on this thread.
pub(crate) fn is_dry_run_active() -> bool {
    DRY_RUN_CONN.get().is_some()
}

/// Returns the current thread's pinned dry-run connection.
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
        .get()
        .expect("DbHandle::DryRun used outside an active dry-run");
    // SAFETY: the pointer was published by `with_dry_run_conn` from a live, locked `&Connection`
    // that outlives every access on this thread; the slot is cleared before that connection is
    // released, so a dangling pointer can never be observed here.
    unsafe { &*ptr }
}

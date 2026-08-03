use std::cell::Cell;

use rusqlite::Connection;

thread_local! {
    static DRY_RUN_CONN: Cell<Option<*const Connection>> = const { Cell::new(None) };
}

pub(crate) fn with_dry_run_conn<T>(conn: &Connection, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<*const Connection>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DRY_RUN_CONN.set(self.0);
        }
    }

    let _restore = Restore(DRY_RUN_CONN.replace(Some(std::ptr::from_ref(conn))));
    f()
}

pub(crate) fn is_dry_run_active() -> bool {
    DRY_RUN_CONN.get().is_some()
}

pub(crate) fn current_dry_run_conn() -> &'static Connection {
    #[allow(clippy::expect_used)]
    let ptr = DRY_RUN_CONN
        .get()
        .expect("DbHandle::DryRun used outside an active dry-run");
    // SAFETY: the pointer was published by `with_dry_run_conn` from a live, locked `&Connection`
    // that outlives every access on this thread; the slot is cleared before that connection is
    // released, so a dangling pointer can never be observed here.
    unsafe { &*ptr }
}

use rusqlite::Connection;

use crate::sqlite::connection::pool::PooledReader;
use crate::sqlite::connection::pragmas::{
    clear_sqlite_progress_handler, install_sqlite_progress_handler,
};
use crate::sqlite::connection::sync::MutexGuard;

pub(crate) enum ReadGuard<'a> {
    Pooled(PooledReader),
    Locked(MutexGuard<'a, Connection>),
    Borrowed(&'a Connection),
}

impl ReadGuard<'_> {
    pub(crate) fn start_operation(&self) {
        install_sqlite_progress_handler(self);
    }
}

impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        clear_sqlite_progress_handler(self);
    }
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

pub(crate) enum WriteGuard<'a> {
    Locked(MutexGuard<'a, Connection>),
    Borrowed(&'a Connection),
}

impl WriteGuard<'_> {
    pub(crate) fn start_operation(&self) {
        install_sqlite_progress_handler(self);
    }
}

impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        clear_sqlite_progress_handler(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connection::pragmas::install_sqlite_progress_handler_with_timeout;
    use rusqlite::Connection;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn dropping_a_guard_clears_its_expired_progress_handler() {
        let conn = Connection::open_in_memory().unwrap();

        {
            let guard = ReadGuard::Borrowed(&conn);
            install_sqlite_progress_handler_with_timeout(&guard, Duration::from_millis(1));
            thread::sleep(Duration::from_millis(5));
        }

        let value: i64 = conn
            .query_row(
                "WITH RECURSIVE counter(x) AS (
                     SELECT 1
                     UNION ALL
                     SELECT x + 1 FROM counter LIMIT 100000
                 )
                 SELECT sum(x) FROM counter",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, 5_000_050_000);
    }
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

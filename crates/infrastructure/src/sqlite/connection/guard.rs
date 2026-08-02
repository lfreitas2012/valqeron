//! RAII guards handed to repositories for reading and writing.
//!
//! Both guards `Deref` to a [`Connection`], so repository code is agnostic to how the connection
//! was obtained (pooled reader, locked writer, or a borrowed dry-run connection).

use rusqlite::Connection;

use crate::sqlite::connection::pool::PooledReader;
use crate::sqlite::connection::sync::MutexGuard;

/// A read guard: either a pooled reader connection (normal operation) or a borrowed connection
/// (inside a dry-run, where all work shares the already-locked writer connection).
pub(crate) enum ReadGuard<'a> {
    Pooled(PooledReader),
    Locked(MutexGuard<'a, Connection>),
    Borrowed(&'a Connection),
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

/// A write guard: either the mutex-locked writer connection (normal operation) or a borrowed
/// connection (inside a dry-run, where the writer mutex is already held by the dry-run driver).
pub(crate) enum WriteGuard<'a> {
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

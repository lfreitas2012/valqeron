//! RAII guards for repository read and write access.
//!
//! Both guards dereference to [`Connection`], regardless of whether the connection is pooled,
//! mutex-locked, or borrowed during a dry-run.

use rusqlite::Connection;

use crate::sqlite::connection::pool::PooledReader;
use crate::sqlite::connection::sync::MutexGuard;

/// A reader connection held by a repository operation.
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

/// A writer connection held by a repository operation.
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

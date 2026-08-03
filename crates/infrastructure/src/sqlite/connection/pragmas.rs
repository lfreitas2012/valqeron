use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::sqlite::connection::sync::{Arc, Mutex};
use crate::sqlite::error::SqliteError;

const MAX_OPERATION_EXECUTION_TIME: Duration = Duration::from_secs(15);

pub(crate) type SharedConnection = Arc<Mutex<Connection>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    #[default]
    Normal,
    Full,
}

impl Synchronous {
    fn as_pragma(self) -> &'static str {
        match self {
            Synchronous::Normal => "NORMAL",
            Synchronous::Full => "FULL",
        }
    }
}

#[derive(Clone)]
pub(crate) enum DbPath {
    File(PathBuf),
    Memory,
}

impl DbPath {
    pub(crate) fn is_memory(&self) -> bool {
        matches!(self, DbPath::Memory)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ConnectionRole {
    Writer,
    Reader,
}

pub(crate) fn open_connection(
    path: &DbPath,
    role: ConnectionRole,
) -> Result<Connection, SqliteError> {
    let map_err = |source| SqliteError::Connection { source };
    match path {
        DbPath::File(p) => {
            let flags = match role {
                ConnectionRole::Writer => {
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
                }
                ConnectionRole::Reader => OpenFlags::SQLITE_OPEN_READ_ONLY,
            };
            Connection::open_with_flags(p, flags).map_err(map_err)
        }
        DbPath::Memory => {
            let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
            Connection::open_in_memory_with_flags(flags).map_err(map_err)
        }
    }
}

pub(crate) fn configure(
    conn: &Connection,
    role: ConnectionRole,
    is_memory: bool,
    synchronous: Synchronous,
    busy_timeout: Duration,
) -> Result<(), SqliteError> {
    debug_assert!(
        !(is_memory && matches!(role, ConnectionRole::Reader)),
        "in-memory databases have no reader pool — reads share the writer connection \
         (see ReaderSource::SharedWithWriter), so this combination should never occur"
    );

    let pragma_err = |source| SqliteError::Pragma { source };

    if !is_memory {
        match role {
            ConnectionRole::Writer => conn
                .pragma_update(None, "journal_mode", "WAL")
                .map_err(pragma_err)?,
            ConnectionRole::Reader => {
                conn.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                    .map_err(pragma_err)?;
            }
        }
    }

    conn.pragma_update(None, "synchronous", synchronous.as_pragma())
        .map_err(pragma_err)?;

    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(pragma_err)?;

    conn.busy_timeout(busy_timeout).map_err(pragma_err)?;

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

    Ok(())
}

pub(crate) fn install_sqlite_progress_handler(conn: &Connection) {
    install_sqlite_progress_handler_with_timeout(conn, MAX_OPERATION_EXECUTION_TIME);
}

pub(crate) fn install_sqlite_progress_handler_with_timeout(conn: &Connection, timeout: Duration) {
    let operation_start = Instant::now();

    let _ = conn.progress_handler(
        5_000,
        Some(move || {
            if Instant::now().duration_since(operation_start) > timeout {
                tracing::error!(
                    "SQLite operation interrupted: execution exceeded time limit of {:?}",
                    timeout
                );
                true
            } else {
                false
            }
        }),
    );
}

pub(crate) fn clear_sqlite_progress_handler(conn: &Connection) {
    let _ = conn.progress_handler(0, None::<fn() -> bool>);
}

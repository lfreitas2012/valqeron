//! SQLite connection setup and operation-scoped SQL execution limits.
//!
//! It opens one physical connection, applies its role-specific configuration, and installs the
//! operation execution limit handler. Pooling and connection lifecycle management belong to
//! [`pool`](crate::sqlite::connection::pool) and [`Database`](crate::sqlite::connection::Database).

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::sqlite::connection::sync::{Arc, Mutex};
use crate::sqlite::error::SqliteError;

/// Maximum wall-clock duration a repository operation may execute SQL before being interrupted.
const MAX_OPERATION_EXECUTION_TIME: Duration = Duration::from_secs(15);

/// A shared connection to the database.
pub(crate) type SharedConnection = Arc<Mutex<Connection>>;

/// SQLite `synchronous` setting used for the writer connection.
///
/// Controls how aggressively the writer flushes to durable storage on commit. Readers are unaffected
/// (they never write). See the durability tradeoff documented on [`configure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    /// `synchronous=NORMAL`.
    ///
    /// Lower commit latency with the risk of losing a committed transaction after a power or OS
    /// failure.
    #[default]
    Normal,

    /// `synchronous=FULL`.
    ///
    /// Synchronizes commits more durably at higher write latency.
    Full,
}

impl Synchronous {
    /// The pragma value string SQLite expects.
    fn as_pragma(self) -> &'static str {
        match self {
            Synchronous::Normal => "NORMAL",
            Synchronous::Full => "FULL",
        }
    }
}

/// Identifies the SQLite database location.
///
/// The variant is retained so connections can select the appropriate open mode, and lifecycle code
/// can determine whether WAL checkpointing applies.
#[derive(Clone)]
pub(crate) enum DbPath {
    File(PathBuf),
    /// An in-memory SQLite database.
    Memory,
}

impl DbPath {
    /// Returns `true` for an in-memory database.
    pub(crate) fn is_memory(&self) -> bool {
        matches!(self, DbPath::Memory)
    }
}

/// The role a connection plays, which selects its open flags and pragma profile.
#[derive(Clone, Copy)]
pub(crate) enum ConnectionRole {
    Writer,
    Reader,
}

/// Opens one SQLite connection with flags appropriate for `role`.
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

/// Applies the SQLite configuration for a connection role.
///
/// | PRAGMA NAME / API | PRAGMA VALUE | Description |
/// |: --- |: --- |: --- |
/// | journal_mode | WAL | Enables SQLite WAL mode on the writer; readers query the resulting mode. Skipped for in-memory databases. Check [SQLite docs](https://www.sqlite.org/wal.html) for more information. |
/// | synchronous | NORMAL (default) or FULL | Configurable via [`DatabaseConfig::synchronous`](crate::sqlite::connection::DatabaseConfig). **NORMAL** (default): fast; a committed transaction can be lost in a power/OS crash, though the database is never corrupted. **FULL**: the database file is fully synchronized on commit so committed transactions survive power loss, at the cost of write latency. <br><br>See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_synchronous) for more information. |
/// | foreign_keys | ON | Enforce foreign key constraints. See [Sqlite docs](https://www.sqlite.org/foreignkeys.html) for more information. |
/// | busy_timeout | Configurable (5 seconds by default) | Wait up to the configured duration when a locked database prevents progress. See [SQLite docs](https://www.sqlite.org/pragma.html#pragma_busy_timeout). |
/// | cache_size | -64000 | The database connection cache is limited to 64MB (64,000 KiB). See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_cache_size) for more information. |
/// | temp_store | MEMORY | Forces temporary tables, indices, and views to be held purely in volatile RAM instead of spilling to disk files. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_temp_store) for more information. |
/// | mmap_size | 268,435,456 | Sets the maximum memory-mapped I/O size to 256 MiB. Skipped for in-memory databases. See [SQLite docs](https://www.sqlite.org/pragma.html#pragma_mmap_size). |
/// | wal_autocheckpoint | 1000 | Runs a PASSIVE checkpoint at 1,000 WAL pages. Skipped for in-memory databases. See [SQLite docs](https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint). |
/// | statement_cache | 64 | Set the maximum number of cached prepared statements this connection will hold. See [rusqlite docs](https://docs.rs/rusqlite/latest/src/rusqlite/cache.rs.html#48) |
/// | query_only | ON / OFF | Enables read-only enforcement for `ConnectionRole::Reader`. See [SQLite docs](https://www.sqlite.org/pragma.html#pragma_query_only). |
///
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

/// Installs a progress handler with an operation-scoped SQL execution budget.
///
/// Guards acquire this handler at the start of a repository operation and clear it when they are
/// dropped. The callback runs every 5,000 SQLite VM instructions and interrupts SQL with
/// `SQLITE_INTERRUPT` after [`MAX_OPERATION_EXECUTION_TIME`].
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
                true // Returns SQLITE_INTERRUPT to rusqlite
            } else {
                false
            }
        }),
    );
}

/// Clears the operation progress handler before a connection is returned to its pool.
pub(crate) fn clear_sqlite_progress_handler(conn: &Connection) {
    let _ = conn.progress_handler(0, None::<fn() -> bool>);
}

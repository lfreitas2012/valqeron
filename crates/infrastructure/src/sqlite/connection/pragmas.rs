//! Low-level connection primitives: opening connections with role-appropriate flags, applying the
//! pragma set, and the per-query wall-clock timeout progress handler.
//!
//! This module holds everything about a *single* physical connection. Multi-connection concerns
//! (the reader pool, the writer mutex) live in [`pool`](crate::sqlite::connection::pool); the
//! [`Database`](crate::sqlite::connection::Database) lifecycle composes both.

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::sqlite::connection::sync::{Arc, Mutex};
use crate::sqlite::error::SqliteDbError;

/// Maximum duration a single SQL query is allowed to run before being aborted.
const MAX_QUERY_EXECUTION_TIME: Duration = Duration::from_secs(15);

/// Minimum idle gap between progress checks to detect a new query invocation.
const IDLE_RESET_THRESHOLD: Duration = Duration::from_millis(50);

/// A shared connection to the database.
pub(crate) type SharedConnection = Arc<Mutex<Connection>>;

/// Writer `synchronous` pragma level.
///
/// Controls how aggressively the writer flushes to durable storage on commit. Readers are unaffected
/// (they never write). See the durability tradeoff documented on [`configure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    /// `synchronous=NORMAL`.
    ///
    /// Fast. A committed transaction can be lost in a power/OS crash, but the database is never corrupted.
    #[default]
    Normal,

    /// `synchronous=FULL`:
    ///
    /// Slower. A committed transaction survives power loss.
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

/// Where the database lives; retained so the writer and reader-pool connections can all be opened
/// against the same location, and so `Drop` knows whether a WAL checkpoint is meaningful.
#[derive(Clone)]
pub(crate) enum DbPath {
    File(PathBuf),
    /// A named in-memory database served by SQLite's `memdb` VFS (available since SQLite 3.36.0).
    /// Multiple connections opened against the same name see the same content for the lifetime of
    /// at least one open handle — without opting the process into shared-cache mode
    /// (https://www.sqlite.org/c3ref/enable_shared_cache.html) and its table-level locking quirks.
    Memory(String),
}

impl DbPath {
    /// Whether this is an in-memory database.
    pub(crate) fn is_memory(&self) -> bool {
        matches!(self, DbPath::Memory(_))
    }
}

/// The role a connection plays, which selects its open flags and pragma profile.
#[derive(Clone, Copy)]
pub(crate) enum ConnectionRole {
    Writer,
    Reader,
}

/// Open a single connection to `path` with flags appropriate for `role`.
pub(crate) fn open_connection(
    path: &DbPath,
    role: ConnectionRole,
) -> Result<Connection, SqliteDbError> {
    let map_err = |source| SqliteDbError::Connection { source };
    match path {
        DbPath::File(p) => {
            // Enforce read-only at the OS/SQLite layer for readers, not just via the `query_only`
            // pragma (which is kept as defense-in-depth in `configure`). Writers may create the file.
            let flags = match role {
                ConnectionRole::Writer => {
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
                }
                ConnectionRole::Reader => OpenFlags::SQLITE_OPEN_READ_ONLY,
            };
            Connection::open_with_flags(p, flags).map_err(map_err)
        }
        DbPath::Memory(_name) => {
            let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
            Connection::open_in_memory_with_flags(flags).map_err(map_err)
        }
    }
}

/// Configures Sqlite database connection for the given role.
///
/// | PRAGMA NAME / API | PRAGMA VALUE | Description |
/// |: --- |: --- |: --- |
/// | journal_mode | WAL | Enables Sqlite WAL mode. Skip for in-memory databases. Check [Sqlite docs](https://www.sqlite.org/wal.html) for more information. |
/// | synchronous | NORMAL (default) or FULL | Configurable via [`DatabaseConfig::synchronous`](crate::sqlite::connection::DatabaseConfig). **NORMAL** (default): fast; a committed transaction can be lost in a power/OS crash, though the database is never corrupted. **FULL**: the database file is fully synchronized on commit so committed transactions survive power loss, at the cost of write latency. <br><br>See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_synchronous) for more information. |
/// | foreign_keys | ON | Enforce foreign key constraints. See [Sqlite docs](https://www.sqlite.org/foreignkeys.html) for more information. |
/// | busy_timeout | 5000 | Abort any operation that takes longer than 5 seconds to complete. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_busy_timeout) for more information. |
/// | cache_size | -64000 | The database connection cache is limited to 64MB (64,000 KiB). See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_cache_size) for more information. |
/// | temp_store | MEMORY | Forces temporary tables, indices, and views to be held purely in volatile RAM instead of spilling to disk files. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_temp_store) for more information. |
/// | mmap_size | 268,435,456 | Sets the maximum memory-mapped I/O budget to 256MB to significantly speed up data read operations. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_mmap_size) for more information. <br><br>Skip for in-memory databases |
/// | wal_autocheckpoint | 1000 | Automatically runs a PASSIVE checkpoint when the WAL log equals or exceeds 1,000 pages. Skip for in-memory databases. See [Sqlite docs](https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint) for more information |
/// | statement_cache | 64 | Set the maximum number of cached prepared statements this connection will hold. See [rusqlite docs](https://docs.rs/rusqlite/latest/src/rusqlite/cache.rs.html#48) |
/// | query_only | ON / OFF | Activates strict read-only mode (`SQLITE_READONLY`) exclusively if the current connection's assigned role is `ConnectionRole::Reader`.  See [the Sqlite docs](https://www.sqlite.org/pragma.html#pragma_query_only) for more information. |
///
pub(crate) fn configure(
    conn: &Connection,
    role: ConnectionRole,
    is_memory: bool,
    synchronous: Synchronous,
) -> Result<(), SqliteDbError> {
    debug_assert!(
        !(is_memory && matches!(role, ConnectionRole::Reader)),
        "in-memory databases have no reader pool — reads share the writer connection \
         (see ReaderSource::SharedWithWriter), so this combination should never occur"
    );

    let pragma_err = |source| SqliteDbError::Pragma { source };

    if !is_memory {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(pragma_err)?;
    }

    // Durability knob. NORMAL (the default) is fast but a committed transaction can be lost on a
    // power/OS crash (the database itself is never corrupted); FULL trades write latency for
    // power-loss durability. Only meaningful on the writer — readers never write.
    conn.pragma_update(None, "synchronous", synchronous.as_pragma())
        .map_err(pragma_err)?;

    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(pragma_err)?;

    conn.busy_timeout(Duration::from_secs(5))
        .map_err(pragma_err)?;

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

    configure_sqlite_progress_handler(conn);

    Ok(())
}

/// Configures a progress handler on `conn` to enforce a wall-clock timeout on individual SQL queries.
///
/// Registers a callback invoked every 5,000 SQLite VM instructions:
/// - Resets the statement timer if elapsed time since the last callback exceeds [`IDLE_RESET_THRESHOLD`],
///   indicating the connection was idle between pool uses.
/// - Aborts the current query with `SQLITE_INTERRUPT` if execution exceeds [`MAX_QUERY_EXECUTION_TIME`].
fn configure_sqlite_progress_handler(conn: &Connection) {
    let mut query_start = Instant::now();
    let mut last_check = Instant::now();

    let _ = conn.progress_handler(
        5_000,
        Some(move || {
            let now = Instant::now();

            // Reset query_start if the connection was idle in the pool
            if now.duration_since(last_check) > IDLE_RESET_THRESHOLD {
                query_start = now;
            }
            last_check = now;

            if now.duration_since(query_start) > MAX_QUERY_EXECUTION_TIME {
                tracing::error!(
                    "SQLite query interrupted: execution exceeded time limit of {:?}",
                    MAX_QUERY_EXECUTION_TIME
                );
                true // Returns SQLITE_INTERRUPT to rusqlite
            } else {
                false
            }
        }),
    );
}

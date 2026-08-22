//! Write-path helpers shared by the SQLite repositories: busy/locked retry
//! with linear backoff, and disambiguation of guarded writes that affected
//! zero rows.

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::StorageFault;
use valqeron_core::common::WriteOutcome;

const BUSY_MAX_ATTEMPTS: u32 = 5;

const BUSY_BACKOFF_BASE: Duration = Duration::from_millis(10);

pub(crate) fn backend(e: rusqlite::Error) -> StorageFault {
    StorageFault::new(e)
}

fn is_busy_or_locked(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::DatabaseBusy
                || e.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

pub(crate) fn with_busy_retry<T>(op: impl Fn() -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    let mut attempt = 0u32;
    loop {
        match op() {
            Err(e) if attempt.saturating_add(1) < BUSY_MAX_ATTEMPTS && is_busy_or_locked(&e) => {
                attempt = attempt.saturating_add(1);
                let backoff = BUSY_BACKOFF_BASE.saturating_mul(attempt);
                tracing::warn!(
                    attempt,
                    backoff_ms = u64::try_from(backoff.as_millis()).unwrap_or(u64::MAX),
                    "database busy/locked; retrying write after backoff"
                );
                std::thread::sleep(backoff);
            }
            other => return other,
        }
    }
}

/// Disambiguates a version-guarded write that affected zero rows by
/// re-reading the row's current version. `version_sql` must select the
/// `version` column by a single `?1` id parameter (e.g.
/// `SELECT version FROM issuer WHERE id = ?1`).
pub(crate) fn write_outcome(
    conn: &Connection,
    version_sql: &str,
    id: &[u8],
    expected_version: u32,
) -> rusqlite::Result<WriteOutcome> {
    let actual: Option<u32> = conn
        .prepare_cached(version_sql)?
        .query_row(params![id], |row| row.get(0))
        .optional()?;

    Ok(match actual {
        Some(actual) => WriteOutcome::VersionMismatch {
            expected: expected_version,
            actual,
        },
        None => WriteOutcome::Missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn busy_error() -> rusqlite::Error {
        let ffi = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY);
        rusqlite::Error::SqliteFailure(ffi, Some("database is locked".into()))
    }

    #[test]
    fn with_busy_retry_succeeds_after_transient_busy() {
        let attempts = Cell::new(0u32);
        let result = with_busy_retry(|| {
            let n = attempts.get() + 1;
            attempts.set(n);
            if n < 3 { Err(busy_error()) } else { Ok(42) }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.get(), 3, "should have retried until success");
    }

    #[test]
    fn with_busy_retry_gives_up_after_max_attempts() {
        let attempts = Cell::new(0u32);
        let result: rusqlite::Result<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(busy_error()) // always busy
        });
        assert!(result.is_err());
        assert_eq!(
            attempts.get(),
            BUSY_MAX_ATTEMPTS,
            "should stop after BUSY_MAX_ATTEMPTS"
        );
    }

    #[test]
    fn with_busy_retry_does_not_retry_non_busy_errors() {
        let attempts = Cell::new(0u32);
        let result: rusqlite::Result<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(rusqlite::Error::QueryReturnedNoRows)
        });
        assert!(matches!(result, Err(rusqlite::Error::QueryReturnedNoRows)));
        assert_eq!(attempts.get(), 1, "non-busy errors must not be retried");
    }
}

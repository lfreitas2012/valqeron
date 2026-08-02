//! Generic, entity-agnostic row-mapping primitives for the SQLite driver.
//!
//! [`FromRow`] is the infra-local analogue of a "row → typed value" mapper (comparable to sqlx's
//! `FromRow`, but synchronous for rusqlite). Each entity's row model (e.g.
//! [`IssuerRow`](crate::sqlite::issuer::model::IssuerRow)) implements it to reconstitute a domain
//! object from a query row, using per-entity column converters that build on the helpers here.

use rusqlite::Row;
use rusqlite::types::Type;

/// Maps a single query [`Row`] into `Self`.
///
/// Implementors read columns by name and convert them into domain types, returning a
/// [`rusqlite::Error`] (typically [`rusqlite::Error::FromSqlConversionFailure`]) when a value
/// cannot be converted.
pub trait FromRow: Sized {
    /// Build `Self` from `row`.
    ///
    /// # Errors
    ///
    /// Returns an error if a column is missing or cannot be converted into the target domain type.
    fn from_row(row: &Row) -> rusqlite::Result<Self>;
}

/// Wrap a conversion error as a rusqlite column-conversion failure.
pub(crate) fn conversion_failure(
    column: usize,
    kind: Type,
    err: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, kind, Box::new(err))
}

/// Resolve a column's positional index for accurate error reporting, defaulting to 0 if the name is
/// somehow absent (the subsequent `get` would surface the real error anyway).
pub(crate) fn column_index(row: &Row, name: &str) -> usize {
    row.as_ref().column_index(name).unwrap_or(0)
}

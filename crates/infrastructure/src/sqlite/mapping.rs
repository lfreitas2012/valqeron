//! Row-mapping abstraction for the SQLite backend.
//!
//! [`FromRow`] is the infra-local analogue of a "row → typed value" mapper
//! (comparable to sqlx's `FromRow`, but synchronous for rusqlite). Model
//! newtypes in [`crate::sqlite::models`] implement it to reconstitute domain
//! objects from a query row. The `column_*` helpers centralize the fallible
//! conversions between SQLite column values and domain value objects so the
//! model implementations stay declarative.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use ftracker_identifiers::{Cnpj, CountryCode, Lei};
use rusqlite::Row;
use rusqlite::types::Type;
use valqeron_core::{IssuerId, IssuerName, IssuerStatus};

/// Maps a single query [`Row`] into `Self`.
///
/// Implementors read columns by name and convert them into domain types,
/// returning a [`rusqlite::Error`] (typically
/// [`rusqlite::Error::FromSqlConversionFailure`]) when a value cannot be
/// converted.
pub trait FromRow: Sized {
    /// Build `Self` from `row`.
    ///
    /// # Errors
    ///
    /// Returns an error if a column is missing or cannot be converted into the
    /// target domain type.
    fn from_row(row: &Row) -> rusqlite::Result<Self>;
}

/// Wrap a conversion error as a rusqlite column-conversion failure.
fn conversion_failure(
    column: usize,
    kind: Type,
    err: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, kind, Box::new(err))
}

/// Read a 16-byte BLOB `id` column into an [`IssuerId`].
pub fn column_issuer_id(row: &Row, name: &str) -> rusqlite::Result<IssuerId> {
    let bytes: Vec<u8> = row.get(name)?;
    let idx = row.as_ref().column_index(name).unwrap_or(0);
    let uuid =
        uuid::Uuid::from_slice(&bytes).map_err(|e| conversion_failure(idx, Type::Blob, e))?;
    Ok(IssuerId::from_uuid(uuid))
}

/// Read a TEXT `status` column into an [`IssuerStatus`].
pub fn column_status(row: &Row, name: &str) -> rusqlite::Result<IssuerStatus> {
    let raw: String = row.get(name)?;
    IssuerStatus::from_str(&raw).map_err(|e| conversion_failure(2, Type::Text, e))
}

/// Read an RFC 3339 TEXT `created_at` column into a UTC [`DateTime`].
pub fn column_datetime(row: &Row, name: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(name)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| conversion_failure(3, Type::Text, e))
}

/// Read an optional TEXT `name` column into an [`IssuerName`].
pub fn column_opt_name(row: &Row, name: &str) -> rusqlite::Result<Option<IssuerName>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(IssuerName::new)
        .transpose()
        .map_err(|e| conversion_failure(1, Type::Text, e))
}

/// Read an optional TEXT `cnpj` column into a [`Cnpj`].
pub fn column_opt_cnpj(row: &Row, name: &str) -> rusqlite::Result<Option<Cnpj>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Cnpj::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(4, Type::Text, e))
}

/// Read an optional TEXT `lei` column into a [`Lei`].
pub fn column_opt_lei(row: &Row, name: &str) -> rusqlite::Result<Option<Lei>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Lei::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(5, Type::Text, e))
}

/// Read an optional TEXT `country_code` column into a [`CountryCode`].
pub fn column_opt_country_code(row: &Row, name: &str) -> rusqlite::Result<Option<CountryCode>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| CountryCode::from_str(&s))
        .transpose()
        .map_err(|e| conversion_failure(6, Type::Text, e))
}

/// Render an [`IssuerStatus`] as its TEXT column representation.
pub fn status_as_str(status: IssuerStatus) -> String {
    status.into()
}

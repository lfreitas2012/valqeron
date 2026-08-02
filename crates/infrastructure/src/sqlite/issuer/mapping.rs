//! SQLite-to-domain conversions for issuer columns.
//!
//! These helpers keep fallible column conversions out of
//! [`IssuerRow`](crate::sqlite::issuer::model::IssuerRow) and use the generic row-mapping helpers
//! in [`crate::sqlite::row`].

use std::str::FromStr;

use chrono::{DateTime, Utc};
use ftracker_identifiers::{Cnpj, CountryCode, Lei};
use rusqlite::Row;
use rusqlite::types::Type;
use valqeron_core::{IssuerId, IssuerName, IssuerStatus};

use crate::sqlite::row::{column_index, conversion_failure};

/// Reads a 16-byte BLOB `id` column as an [`IssuerId`].
pub(crate) fn column_issuer_id(row: &Row, name: &str) -> rusqlite::Result<IssuerId> {
    let bytes: Vec<u8> = row.get(name)?;
    let idx = column_index(row, name);
    let uuid =
        uuid::Uuid::from_slice(&bytes).map_err(|e| conversion_failure(idx, Type::Blob, e))?;
    Ok(IssuerId::from_uuid(uuid))
}

/// Reads a TEXT `status` column as an [`IssuerStatus`].
pub(crate) fn column_status(row: &Row, name: &str) -> rusqlite::Result<IssuerStatus> {
    let raw: String = row.get(name)?;
    IssuerStatus::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Reads an RFC 3339 TEXT `created_at` column as a UTC [`DateTime`].
pub(crate) fn column_datetime(row: &Row, name: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(name)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Reads an optional TEXT `name` column as an [`IssuerName`].
pub(crate) fn column_opt_name(row: &Row, name: &str) -> rusqlite::Result<Option<IssuerName>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(IssuerName::new)
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Reads an optional TEXT `cnpj` column as a [`Cnpj`].
pub(crate) fn column_opt_cnpj(row: &Row, name: &str) -> rusqlite::Result<Option<Cnpj>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Cnpj::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Reads an optional TEXT `lei` column as a [`Lei`].
pub(crate) fn column_opt_lei(row: &Row, name: &str) -> rusqlite::Result<Option<Lei>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Lei::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Reads an optional TEXT `country_code` column as a [`CountryCode`].
pub(crate) fn column_opt_country_code(
    row: &Row,
    name: &str,
) -> rusqlite::Result<Option<CountryCode>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| CountryCode::from_str(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

/// Renders an [`IssuerStatus`] as its SQLite TEXT representation.
pub(crate) fn status_as_str(status: IssuerStatus) -> String {
    status.into()
}

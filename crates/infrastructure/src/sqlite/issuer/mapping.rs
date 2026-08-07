use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::Row;
use rusqlite::types::Type;
use valqeron_core::{IssuerId, IssuerName, IssuerStatus};
use valqeron_identifiers::{Cnpj, CountryCode, Lei};

use crate::sqlite::row::{column_index, conversion_failure};

pub(crate) fn column_issuer_id(row: &Row, name: &str) -> rusqlite::Result<IssuerId> {
    let bytes: Vec<u8> = row.get(name)?;
    let idx = column_index(row, name);
    let uuid =
        uuid::Uuid::from_slice(&bytes).map_err(|e| conversion_failure(idx, Type::Blob, e))?;
    Ok(IssuerId::from_uuid(uuid))
}

pub(crate) fn column_status(row: &Row, name: &str) -> rusqlite::Result<IssuerStatus> {
    let raw: String = row.get(name)?;
    IssuerStatus::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_datetime(row: &Row, name: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(name)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_name(row: &Row, name: &str) -> rusqlite::Result<Option<IssuerName>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(IssuerName::new)
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_cnpj(row: &Row, name: &str) -> rusqlite::Result<Option<Cnpj>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Cnpj::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_lei(row: &Row, name: &str) -> rusqlite::Result<Option<Lei>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Lei::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_country_code(
    row: &Row,
    name: &str,
) -> rusqlite::Result<Option<CountryCode>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| CountryCode::from_str(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn status_as_str(status: IssuerStatus) -> String {
    status.into()
}

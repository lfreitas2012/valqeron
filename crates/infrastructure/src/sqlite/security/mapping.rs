use std::str::FromStr;

use rusqlite::Row;
use rusqlite::types::Type;
use valqeron_core::{DrRatio, IssuerId, SecurityId, SecurityKind, SecurityName, SecurityStatus};
use valqeron_identifiers::{Cfi, Isin};

use crate::sqlite::row::{column_index, column_opt_uuid, column_uuid, conversion_failure};

#[derive(Debug, thiserror::Error)]
#[error("dr_ratio columns must be both set or both NULL")]
struct DrRatioPairError;

pub(crate) fn column_security_id(row: &Row, name: &str) -> rusqlite::Result<SecurityId> {
    column_uuid(row, name).map(SecurityId::from_uuid)
}

pub(crate) fn column_opt_security_id(
    row: &Row,
    name: &str,
) -> rusqlite::Result<Option<SecurityId>> {
    column_opt_uuid(row, name).map(|opt| opt.map(SecurityId::from_uuid))
}

pub(crate) fn column_issuer_id(row: &Row, name: &str) -> rusqlite::Result<IssuerId> {
    column_uuid(row, name).map(IssuerId::from_uuid)
}

pub(crate) fn column_kind(row: &Row, name: &str) -> rusqlite::Result<SecurityKind> {
    let raw: String = row.get(name)?;
    SecurityKind::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_status(row: &Row, name: &str) -> rusqlite::Result<SecurityStatus> {
    let raw: String = row.get(name)?;
    SecurityStatus::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_name(row: &Row, name: &str) -> rusqlite::Result<Option<SecurityName>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(SecurityName::new)
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_isin(row: &Row, name: &str) -> rusqlite::Result<Option<Isin>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Isin::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_cfi(row: &Row, name: &str) -> rusqlite::Result<Option<Cfi>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Cfi::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

pub(crate) fn column_opt_dr_ratio(
    row: &Row,
    receipts_name: &str,
    underlying_name: &str,
) -> rusqlite::Result<Option<DrRatio>> {
    let receipts: Option<u32> = row.get(receipts_name)?;
    let underlying: Option<u32> = row.get(underlying_name)?;
    let idx = column_index(row, receipts_name);

    match (receipts, underlying) {
        (Some(receipts), Some(underlying)) => DrRatio::new(receipts, underlying)
            .map(Some)
            .map_err(|e| conversion_failure(idx, Type::Integer, e)),
        (None, None) => Ok(None),
        _ => Err(conversion_failure(idx, Type::Integer, DrRatioPairError)),
    }
}

pub(crate) fn status_as_str(status: SecurityStatus) -> String {
    status.into()
}

pub(crate) fn kind_as_str(kind: SecurityKind) -> String {
    kind.into()
}

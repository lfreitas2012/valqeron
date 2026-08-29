use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::domain::issuer::IssuerId;
use valqeron_core::domain::security::{Security, SecurityId, SecurityName, SecurityPatch};
use valqeron_identifiers::Isin;

use crate::sqlite::row::{FromRow, canonical_timestamp};
use crate::sqlite::security::mapping::{kind_as_str, status_as_str};
use crate::sqlite::security::model::SecurityRow;

pub(crate) const SECURITY_COLUMNS: &str = "id, issuer_id, name, kind, status, created_at, isin, \
     cfi, underlying_security_id, dr_ratio_receipts, dr_ratio_underlying, version";

/// `SECURITY_COLUMNS` with every column qualified by `alias`, for queries
/// that join `security` against another relation carrying clashing column
/// names.
pub(crate) fn security_columns_qualified(alias: &str) -> String {
    SECURITY_COLUMNS
        .split(", ")
        .map(|column| format!("{alias}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn find_by_id(
    conn: &Connection,
    id: &SecurityId,
) -> rusqlite::Result<Option<SecurityRow>> {
    let sql = format!("SELECT {SECURITY_COLUMNS} FROM security WHERE id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![id.as_bytes()], SecurityRow::from_row)
        .optional()
}

pub(crate) fn find_by_isin(
    conn: &Connection,
    isin: &Isin,
) -> rusqlite::Result<Option<SecurityRow>> {
    let sql = format!("SELECT {SECURITY_COLUMNS} FROM security WHERE isin = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![isin.as_str()], SecurityRow::from_row)
        .optional()
}

pub(crate) fn list_all(conn: &Connection) -> rusqlite::Result<Vec<SecurityRow>> {
    let sql = format!("SELECT {SECURITY_COLUMNS} FROM security ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map([], SecurityRow::from_row)?.collect()
}

pub(crate) fn list_by_issuer(
    conn: &Connection,
    issuer_id: &IssuerId,
) -> rusqlite::Result<Vec<SecurityRow>> {
    let sql = format!("SELECT {SECURITY_COLUMNS} FROM security WHERE issuer_id = ?1 ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map(params![issuer_id.as_bytes()], SecurityRow::from_row)?
        .collect()
}

pub(crate) fn list_paged(
    conn: &Connection,
    after: Option<&SecurityId>,
    limit: u32,
) -> rusqlite::Result<Vec<SecurityRow>> {
    match after {
        Some(id) => {
            let sql = format!(
                "SELECT {SECURITY_COLUMNS} FROM security WHERE id > ?1 ORDER BY id LIMIT ?2"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![id.as_bytes(), limit], SecurityRow::from_row)?
                .collect()
        }
        None => {
            let sql = format!("SELECT {SECURITY_COLUMNS} FROM security ORDER BY id LIMIT ?1");
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![limit], SecurityRow::from_row)?
                .collect()
        }
    }
}

pub(crate) fn exists(conn: &Connection, id: &SecurityId) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM security WHERE id = ?1")?;
    stmt.query_row(params![id.as_bytes()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

pub(crate) fn exists_by_isin(conn: &Connection, isin: &Isin) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM security WHERE isin = ?1")?;
    stmt.query_row(params![isin.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

pub(crate) fn insert(conn: &Connection, security: &Security) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO security (id, issuer_id, name, kind, status, created_at, isin, cfi,
                               underlying_security_id, dr_ratio_receipts, dr_ratio_underlying)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    stmt.execute(params![
        security.id().as_bytes(),
        security.issuer_id().as_bytes(),
        security.name().map(SecurityName::as_str),
        kind_as_str(security.kind()),
        status_as_str(security.status()),
        canonical_timestamp(security.created_at()),
        security.isin().map(|i| i.as_str()),
        security.cfi().map(|c| c.as_str()),
        security.underlying_security_id().map(SecurityId::as_bytes),
        security.dr_ratio().map(|r| r.receipts().get()),
        security.dr_ratio().map(|r| r.underlying().get()),
    ])
}

pub(crate) fn apply_patch(
    conn: &Connection,
    id: &SecurityId,
    expected_version: u32,
    patch: &SecurityPatch,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "UPDATE security SET
            name = COALESCE(?2, name),
            status = COALESCE(?3, status),
            isin = COALESCE(?4, isin),
            cfi = COALESCE(?5, cfi),
            dr_ratio_receipts = COALESCE(?6, dr_ratio_receipts),
            dr_ratio_underlying = COALESCE(?7, dr_ratio_underlying),
            version = version + 1
         WHERE id = ?1 AND version = ?8",
    )?;
    stmt.execute(params![
        id.as_bytes(),
        patch.name().map(SecurityName::as_str),
        patch.status().map(status_as_str),
        patch.isin().map(|i| i.as_str()),
        patch.cfi().map(|c| c.as_str()),
        patch.dr_ratio().map(|r| r.receipts().get()),
        patch.dr_ratio().map(|r| r.underlying().get()),
        expected_version,
    ])
}

/// Full replace of the mutable state, guarded by version. Identity facts —
/// `issuer_id`, `kind`, the underlying security link, and `created_at` —
/// are immutable (mirroring `SecurityPatch`) and deliberately not written.
pub(crate) fn update(
    conn: &Connection,
    security: &Security,
    expected_version: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "UPDATE security SET
            name = ?2,
            status = ?3,
            isin = ?4,
            cfi = ?5,
            dr_ratio_receipts = ?6,
            dr_ratio_underlying = ?7,
            version = version + 1
         WHERE id = ?1 AND version = ?8",
    )?;
    stmt.execute(params![
        security.id().as_bytes(),
        security.name().map(SecurityName::as_str),
        status_as_str(security.status()),
        security.isin().map(|i| i.as_str()),
        security.cfi().map(|c| c.as_str()),
        security.dr_ratio().map(|r| r.receipts().get()),
        security.dr_ratio().map(|r| r.underlying().get()),
        expected_version,
    ])
}

pub(crate) fn delete(
    conn: &Connection,
    id: &SecurityId,
    expected_version: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached("DELETE FROM security WHERE id = ?1 AND version = ?2")?;
    stmt.execute(params![id.as_bytes(), expected_version])
}

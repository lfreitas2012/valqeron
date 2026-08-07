use chrono::SecondsFormat;
use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::{Issuer, IssuerId, IssuerName, IssuerPatch};
use valqeron_identifiers::{Cnpj, Lei};

use crate::sqlite::issuer::mapping::status_as_str;
use crate::sqlite::issuer::model::IssuerRow;
use crate::sqlite::row::FromRow;

const ISSUER_COLUMNS: &str = "id, name, status, created_at, cnpj, lei, country_code, version";

fn canonical_timestamp(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn find_by_id(conn: &Connection, id: &IssuerId) -> rusqlite::Result<Option<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer WHERE id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![id.as_bytes()], IssuerRow::from_row)
        .optional()
}

pub(crate) fn list_all(conn: &Connection) -> rusqlite::Result<Vec<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map([], IssuerRow::from_row)?.collect()
}

pub(crate) fn list_paged(
    conn: &Connection,
    after: Option<&IssuerId>,
    limit: u32,
) -> rusqlite::Result<Vec<IssuerRow>> {
    match after {
        Some(id) => {
            let sql =
                format!("SELECT {ISSUER_COLUMNS} FROM issuer WHERE id > ?1 ORDER BY id LIMIT ?2");
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![id.as_bytes(), limit], IssuerRow::from_row)?
                .collect()
        }
        None => {
            let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer ORDER BY id LIMIT ?1");
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![limit], IssuerRow::from_row)?
                .collect()
        }
    }
}

pub(crate) fn exists(conn: &Connection, id: &IssuerId) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")?;
    stmt.query_row(params![id.as_bytes()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

pub(crate) fn exists_by_cnpj(conn: &Connection, cnpj: &Cnpj) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE cnpj = ?1")?;
    stmt.query_row(params![cnpj.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

pub(crate) fn exists_by_lei(conn: &Connection, lei: &Lei) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE lei = ?1")?;
    stmt.query_row(params![lei.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

pub(crate) fn insert(conn: &Connection, issuer: &Issuer) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO issuer (id, name, status, created_at, cnpj, lei, country_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    stmt.execute(params![
        issuer.id().as_bytes(),
        issuer.name().map(IssuerName::as_str),
        status_as_str(issuer.status()),
        canonical_timestamp(issuer.created_at()),
        issuer.cnpj().map(|c| c.as_str()),
        issuer.lei().map(|l| l.as_str()),
        issuer.country_code().map(|c| c.as_str()),
    ])
}

pub(crate) fn apply_patch(
    conn: &Connection,
    id: &IssuerId,
    expected_version: u32,
    patch: &IssuerPatch,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "UPDATE issuer SET
            name = COALESCE(?2, name),
            status = COALESCE(?3, status),
            cnpj = COALESCE(?4, cnpj),
            lei = COALESCE(?5, lei),
            country_code = COALESCE(?6, country_code),
            version = version + 1
         WHERE id = ?1 AND version = ?7",
    )?;
    stmt.execute(params![
        id.as_bytes(),
        patch.name().map(IssuerName::as_str),
        patch.status().map(status_as_str),
        patch.cnpj().map(|c| c.as_str()),
        patch.lei().map(|l| l.as_str()),
        patch.country_code().map(|c| c.as_str()),
        expected_version,
    ])
}

pub(crate) fn update(
    conn: &Connection,
    issuer: &Issuer,
    expected_version: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached(
        "UPDATE issuer SET
            name = ?2,
            status = ?3,
            cnpj = ?4,
            lei = ?5,
            country_code = ?6,
            version = version + 1
         WHERE id = ?1 AND version = ?7",
    )?;
    stmt.execute(params![
        issuer.id().as_bytes(),
        issuer.name().map(IssuerName::as_str),
        status_as_str(issuer.status()),
        issuer.cnpj().map(|c| c.as_str()),
        issuer.lei().map(|l| l.as_str()),
        issuer.country_code().map(|c| c.as_str()),
        expected_version,
    ])
}

pub(crate) fn delete(
    conn: &Connection,
    id: &IssuerId,
    expected_version: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached("DELETE FROM issuer WHERE id = ?1 AND version = ?2")?;
    stmt.execute(params![id.as_bytes(), expected_version])
}

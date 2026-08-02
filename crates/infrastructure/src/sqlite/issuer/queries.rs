//! Stateless SQL statements for the `issuer` table.
//!
//! Each function is a single, focused SQL operation: it prepares a (cached) statement, binds
//! parameters, and returns a raw rusqlite result — either a row model
//! ([`IssuerRow`](crate::sqlite::issuer::model::IssuerRow)) or an affected-row count. This layer
//! holds **all** the SQL and nothing else: it applies no conflict/not-found policy and performs no
//! error wrapping. Those concerns belong to
//! [`SqliteIssuerRepository`](crate::sqlite::issuer::repository::SqliteIssuerRepository), which
//! composes these functions.
//!
//! Functions take `&rusqlite::Connection`, which both a pooled reader guard and the writer guard
//! dereference to, so the same query works for reads and writes.

use chrono::SecondsFormat;
use ftracker_identifiers::{Cnpj, Lei};
use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::{Issuer, IssuerId, IssuerName, IssuerPatch};

use crate::sqlite::issuer::mapping::status_as_str;
use crate::sqlite::issuer::model::IssuerRow;
use crate::sqlite::row::FromRow;

/// Columns projected for every issuer selection, in [`IssuerRow`]'s expected order.
const ISSUER_COLUMNS: &str = "id, name, status, created_at, cnpj, lei, country_code, version";

/// Render a UTC timestamp in the canonical storage form: always `Z`-suffixed, fixed millisecond
/// precision, so stored `created_at` values are lexicographically sortable and directly comparable.
///
/// (Reads remain tolerant of the older `+00:00`/variable-precision form via
/// [`column_datetime`](crate::sqlite::issuer::mapping::column_datetime), so pre-existing rows still
/// parse.)
fn canonical_timestamp(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Fetch an issuer with its version, or `None` if absent.
pub(crate) fn find_by_id(conn: &Connection, id: &IssuerId) -> rusqlite::Result<Option<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer WHERE id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![id.as_bytes()], IssuerRow::from_row)
        .optional()
}

/// Fetch all issuers, ordered by ID.
pub(crate) fn list_all(conn: &Connection) -> rusqlite::Result<Vec<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map([], IssuerRow::from_row)?.collect()
}

/// Fetch one keyset page of issuers ordered by id.
///
/// Returns up to `limit` rows whose `id` sorts strictly after `after` (all rows when `after` is
/// `None`). Ordering is by the `id` primary key, so this is index-backed (no full scan). Callers
/// paginate by passing the last returned id as `after` on the next call.
pub(crate) fn list_paged(
    conn: &Connection,
    after: Option<&IssuerId>,
    limit: u32,
) -> rusqlite::Result<Vec<IssuerRow>> {
    // `?1 IS NULL` short-circuits the id filter for the first page; otherwise seek past `after`.
    let sql = format!(
        "SELECT {ISSUER_COLUMNS} FROM issuer \
         WHERE ?1 IS NULL OR id > ?1 \
         ORDER BY id LIMIT ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let after_bytes = after.map(|id| id.as_bytes());
    stmt.query_map(params![after_bytes, limit], IssuerRow::from_row)?
        .collect()
}

/// Whether an issuer with `id` exists.
pub(crate) fn exists(conn: &Connection, id: &IssuerId) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")?;
    stmt.query_row(params![id.as_bytes()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// Whether any issuer already holds `cnpj`.
pub(crate) fn exists_by_cnpj(conn: &Connection, cnpj: &Cnpj) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE cnpj = ?1")?;
    stmt.query_row(params![cnpj.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// Whether any issuer already holds `lei`.
pub(crate) fn exists_by_lei(conn: &Connection, lei: &Lei) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE lei = ?1")?;
    stmt.query_row(params![lei.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// Insert a new issuer. Returns the number of affected rows (1 on success).
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

/// Apply a partial update guarded by `expected_version`, bumping the version.
///
/// Unset patch fields are left unchanged (`COALESCE`). Returns the number of
/// affected rows (0 means the version guard did not match or the row is absent).
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

/// Fully replace an issuer's mutable fields (clearing unset optionals to NULL),
/// guarded by `expected_version`, bumping the version. `id` and `created_at` are
/// immutable and left untouched. Returns the number of affected rows.
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

/// Delete an issuer guarded by `expected_version`. Returns the number of affected rows.
pub(crate) fn delete(
    conn: &Connection,
    id: &IssuerId,
    expected_version: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached("DELETE FROM issuer WHERE id = ?1 AND version = ?2")?;
    stmt.execute(params![id.as_bytes(), expected_version])
}

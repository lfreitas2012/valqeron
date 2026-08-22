use crate::sqlite::database::{Db, DbHandle};
use crate::sqlite::repositories::securities::{
    SecurityRow, list_by_issuer, security_columns_qualified,
};
use crate::sqlite::row::{
    FromRow, canonical_timestamp, column_datetime, column_index, column_uuid, conversion_failure,
};
use crate::sqlite::support::{backend, with_busy_retry, write_outcome};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;
use valqeron_core::Cnpj;
use valqeron_core::common::{LoadMode, Loading, RepositoryResult, Versioned, WriteOutcome};
use valqeron_core::domain::issuer::{
    Issuer, IssuerId, IssuerName, IssuerPatch, IssuerRepository, IssuerSnapshot, IssuerStatus,
};
use valqeron_core::domain::security::Security;
use valqeron_core::identifiers::{CountryCode, Lei};

// ======================== MODEL ========================
/// One `issuer` row, mapped to a snapshot rather than the entity so the
/// repository can decide how to satisfy the requested
/// [`valqeron_core::LoadMode`]: reconstitute immediately (lazy,
/// `Loading::NotLoaded`) or attach the batch-loaded securities first
/// (eager).
#[derive(Debug)]
pub(crate) struct IssuerRow(pub Versioned<IssuerSnapshot>);

impl IssuerRow {
    pub(crate) fn into_inner(self) -> Versioned<IssuerSnapshot> {
        self.0
    }
}

impl FromRow for IssuerRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let snapshot = IssuerSnapshot {
            id: column_issuer_id(row, "id")?,
            status: column_status(row, "status")?,
            created_at: column_datetime(row, "created_at")?,
            name: column_opt_name(row, "name")?,
            cnpj: column_opt_cnpj(row, "cnpj")?,
            lei: column_opt_lei(row, "lei")?,
            country_code: column_opt_country_code(row, "country_code")?,
            securities: Loading::NotLoaded,
        };
        let version: u32 = row.get("version")?;

        Ok(Self(Versioned {
            data: snapshot,
            version,
        }))
    }
}

// ======================== REPOSITORY ========================
const ISSUER_VERSION_SQL: &str = "SELECT version FROM issuer WHERE id = ?1";

pub struct SqliteIssuerRepository {
    db: DbHandle,
}

impl SqliteIssuerRepository {
    pub(crate) fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

/// Reconstitutes without touching the securities relation
/// ([`Loading::NotLoaded`], as mapped by `IssuerRow`).
fn reconstitute_lazy(row: IssuerRow) -> Versioned<Issuer> {
    let Versioned { data, version } = row.into_inner();
    Versioned {
        data: Issuer::reconstitute(data),
        version,
    }
}

fn reconstitute_with(
    mut snapshot: Versioned<IssuerSnapshot>,
    securities: Vec<Security>,
) -> Versioned<Issuer> {
    snapshot.data.securities = Loading::Loaded(securities);
    Versioned {
        data: Issuer::reconstitute(snapshot.data),
        version: snapshot.version,
    }
}

/// Attaches one batched securities result to many issuer rows. Issuers
/// without a match get `Loaded(vec![])`; securities whose issuer is not in
/// `rows` (possible when a concurrent write lands between the two read
/// statements) are dropped.
fn hydrate_rows(rows: Vec<IssuerRow>, securities: Vec<SecurityRow>) -> Vec<Versioned<Issuer>> {
    let mut grouped: HashMap<Uuid, Vec<Security>> = HashMap::new();
    for row in securities {
        let security = row.into_inner().data;
        grouped
            .entry(*security.issuer_id().as_uuid())
            .or_default()
            .push(security);
    }

    rows.into_iter()
        .map(|row| {
            let snapshot = row.into_inner();
            let securities = grouped
                .remove(snapshot.data.id.as_uuid())
                .unwrap_or_default();
            reconstitute_with(snapshot, securities)
        })
        .collect()
}

impl IssuerRepository for SqliteIssuerRepository {
    fn find_by_id(
        &self,
        id: &IssuerId,
        mode: LoadMode,
    ) -> RepositoryResult<Option<Versioned<Issuer>>> {
        let conn = self.db.read();
        let Some(row) = find_by_id(&conn, id).map_err(backend)? else {
            return Ok(None);
        };

        match mode {
            LoadMode::Lazy => Ok(Some(reconstitute_lazy(row))),
            LoadMode::Eager => {
                let securities = list_by_issuer(&conn, id)
                    .map_err(backend)?
                    .into_iter()
                    .map(|row| row.into_inner().data)
                    .collect();
                Ok(Some(reconstitute_with(row.into_inner(), securities)))
            }
        }
    }

    fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>> {
        let conn = self.db.read();
        let rows = list_all(&conn).map_err(backend)?;

        match mode {
            LoadMode::Lazy => Ok(rows.into_iter().map(reconstitute_lazy).collect()),
            LoadMode::Eager => {
                let securities = securities_for_all_issuers(&conn).map_err(backend)?;
                Ok(hydrate_rows(rows, securities))
            }
        }
    }

    fn list_paged(
        &self,
        after: Option<IssuerId>,
        limit: u32,
        mode: LoadMode,
    ) -> RepositoryResult<Vec<Versioned<Issuer>>> {
        let conn = self.db.read();
        let rows = list_paged(&conn, after.as_ref(), limit).map_err(backend)?;

        match mode {
            LoadMode::Lazy => Ok(rows.into_iter().map(reconstitute_lazy).collect()),
            LoadMode::Eager => {
                let securities =
                    securities_for_issuer_page(&conn, after.as_ref(), limit).map_err(backend)?;
                Ok(hydrate_rows(rows, securities))
            }
        }
    }

    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
        let conn = self.db.read();
        exists(&conn, id).map_err(backend)
    }

    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool> {
        let conn = self.db.read();
        exists_by_cnpj(&conn, cnpj).map_err(backend)
    }

    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool> {
        let conn = self.db.read();
        exists_by_lei(&conn, lei).map_err(backend)
    }

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            insert(&conn, issuer).map(|_| ())
        })
        .map_err(backend)
    }

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match apply_patch(&conn, id, expected_version, &patch)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        let id = issuer.id();
        with_busy_retry(|| {
            let conn = self.db.write();
            match update(&conn, issuer, expected_version)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match delete(&conn, id, expected_version)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }
}

// ======================== QUERIES ========================
const ISSUER_COLUMNS: &str = "id, name, status, created_at, cnpj, lei, country_code, version";

fn find_by_id(conn: &Connection, id: &IssuerId) -> rusqlite::Result<Option<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer WHERE id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![id.as_bytes()], IssuerRow::from_row)
        .optional()
}

fn list_all(conn: &Connection) -> rusqlite::Result<Vec<IssuerRow>> {
    let sql = format!("SELECT {ISSUER_COLUMNS} FROM issuer ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map([], IssuerRow::from_row)?.collect()
}

fn list_paged(
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

fn exists(conn: &Connection, id: &IssuerId) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")?;
    stmt.query_row(params![id.as_bytes()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

fn exists_by_cnpj(conn: &Connection, cnpj: &Cnpj) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE cnpj = ?1")?;
    stmt.query_row(params![cnpj.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

fn exists_by_lei(conn: &Connection, lei: &Lei) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM issuer WHERE lei = ?1")?;
    stmt.query_row(params![lei.as_str()], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

fn insert(conn: &Connection, issuer: &Issuer) -> rusqlite::Result<usize> {
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

fn apply_patch(
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

fn update(conn: &Connection, issuer: &Issuer, expected_version: u32) -> rusqlite::Result<usize> {
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

fn delete(conn: &Connection, id: &IssuerId, expected_version: u32) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare_cached("DELETE FROM issuer WHERE id = ?1 AND version = ?2")?;
    stmt.execute(params![id.as_bytes(), expected_version])
}

// Eager-hydration companions: each read shape has exactly one batched
// securities query, so hydrating N issuers always costs 2 statements
// (issuers + securities), never 1 + N.

/// Securities of every issuer, ordered by owner for in-memory grouping.
/// Companion to `list_all`.
fn securities_for_all_issuers(conn: &Connection) -> rusqlite::Result<Vec<SecurityRow>> {
    let columns = security_columns_qualified("s");
    let sql = format!("SELECT {columns} FROM security s ORDER BY s.issuer_id, s.id");
    let mut stmt = conn.prepare_cached(&sql)?;

    stmt.query_map([], SecurityRow::from_row)?.collect()
}

/// Securities of exactly the issuers a `list_paged` call returns, obtained
/// by joining against the identical keyset-page subquery. Companion to
/// `list_paged`.
fn securities_for_issuer_page(
    conn: &Connection,
    after: Option<&IssuerId>,
    limit: u32,
) -> rusqlite::Result<Vec<SecurityRow>> {
    let columns = security_columns_qualified("s");
    match after {
        Some(id) => {
            let sql = format!(
                "SELECT {columns} FROM security s
                 JOIN (SELECT id FROM issuer WHERE id > ?1 ORDER BY id LIMIT ?2) page
                   ON s.issuer_id = page.id
                 ORDER BY s.issuer_id, s.id"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![id.as_bytes(), limit], SecurityRow::from_row)?
                .collect()
        }
        None => {
            let sql = format!(
                "SELECT {columns} FROM security s
                 JOIN (SELECT id FROM issuer ORDER BY id LIMIT ?1) page
                   ON s.issuer_id = page.id
                 ORDER BY s.issuer_id, s.id"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![limit], SecurityRow::from_row)?
                .collect()
        }
    }
}

// ======================== MAPPINGS ========================
fn column_issuer_id(row: &Row, name: &str) -> rusqlite::Result<IssuerId> {
    column_uuid(row, name).map(IssuerId::from_uuid)
}

fn column_status(row: &Row, name: &str) -> rusqlite::Result<IssuerStatus> {
    let raw: String = row.get(name)?;
    IssuerStatus::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

fn column_opt_name(row: &Row, name: &str) -> rusqlite::Result<Option<IssuerName>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(IssuerName::new)
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

fn column_opt_cnpj(row: &Row, name: &str) -> rusqlite::Result<Option<Cnpj>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Cnpj::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

fn column_opt_lei(row: &Row, name: &str) -> rusqlite::Result<Option<Lei>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| Lei::new(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

fn column_opt_country_code(row: &Row, name: &str) -> rusqlite::Result<Option<CountryCode>> {
    let raw: Option<String> = row.get(name)?;
    raw.map(|s| CountryCode::from_str(&s))
        .transpose()
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

fn status_as_str(status: IssuerStatus) -> String {
    status.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::database::{Database, TempDatabase};
    use crate::sqlite::repositories::securities::SqliteSecurityRepository;
    use chrono::Utc;
    use std::str::FromStr;
    use valqeron_core::domain::issuer::{IssuerName, IssuerStatus};
    use valqeron_core::domain::security::{
        SecurityId, SecurityKind, SecurityName, SecurityRepository, SecurityStatus,
    };
    use valqeron_core::identifiers::Isin;

    fn test_repo() -> (TempDatabase, SqliteIssuerRepository) {
        let db = Database::open_temp();
        let repo = SqliteIssuerRepository::new(db.handle());
        (db, repo)
    }

    #[test]
    fn insert_then_find_round_trips_and_defaults_to_version_1() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder()
            .name(IssuerName::new("Acme Corp").unwrap())
            .build()
            .unwrap();

        repo.insert(&issuer).unwrap();

        let found = repo
            .find_by_id(issuer.id(), LoadMode::Lazy)
            .unwrap()
            .expect("Issuer should be found");
        assert_eq!(
            found.version, 1,
            "New insertions should default to version 1"
        );
        assert_eq!(found.data.id(), issuer.id());
    }

    #[test]
    fn apply_patch_bumps_version_on_success() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        assert_eq!(
            repo.apply_patch(issuer.id(), 1, patch).unwrap(),
            WriteOutcome::Applied
        );

        let updated = repo
            .find_by_id(issuer.id(), LoadMode::Lazy)
            .unwrap()
            .unwrap();
        assert!(updated.data.status().is_retired());
        assert_eq!(updated.version, 2, "Version should be incremented to 2");
    }

    #[test]
    fn apply_patch_reports_version_mismatch_on_stale_version() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        let outcome = repo.apply_patch(issuer.id(), 99, patch).unwrap();

        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            },
            "a stale expected version must report the actual current version"
        );
    }

    #[test]
    fn apply_patch_on_missing_id_reports_missing() {
        let (_db, repo) = test_repo();
        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        let outcome = repo.apply_patch(&IssuerId::new(), 1, patch).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn insert_duplicate_id_is_a_storage_fault() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();

        repo.insert(&issuer).unwrap();
        let result = repo.insert(&issuer);

        assert!(result.is_err(), "duplicate id must fail as a storage fault");
    }

    #[test]
    fn delete_with_correct_version_removes_the_row() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        assert_eq!(repo.delete(issuer.id(), 1).unwrap(), WriteOutcome::Applied);
        assert!(
            repo.find_by_id(issuer.id(), LoadMode::Lazy)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn delete_with_stale_version_reports_version_mismatch() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let outcome = repo.delete(issuer.id(), 99).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            },
            "stale-version delete should report a version mismatch, not a silent no-op"
        );
        assert!(
            repo.find_by_id(issuer.id(), LoadMode::Lazy)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn delete_missing_id_reports_missing() {
        let (_db, repo) = test_repo();
        let outcome = repo.delete(&IssuerId::new(), 1).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn dry_run_repository_writes_are_rolled_back() {
        let db = Database::open_temp();
        let issuer = Issuer::builder().build().unwrap();

        db.dry_run(|h| {
            let repo = SqliteIssuerRepository::new(h.clone());
            repo.insert(&issuer).unwrap();
            assert!(
                repo.find_by_id(issuer.id(), LoadMode::Lazy)
                    .unwrap()
                    .is_some()
            );
        })
        .unwrap();

        let repo = SqliteIssuerRepository::new(db.handle());
        assert!(
            repo.find_by_id(issuer.id(), LoadMode::Lazy)
                .unwrap()
                .is_none(),
            "dry-run insert must not persist"
        );
    }

    #[test]
    fn update_replaces_all_fields_and_clears_optionals_to_null() {
        let (_db, repo) = test_repo();
        let id = IssuerId::new();

        let original = Issuer::builder()
            .id(id)
            .name(IssuerName::new("Acme Corp").unwrap())
            .lei(Lei::new("5493000IBP32UQZ0KL24").unwrap())
            .build()
            .unwrap();
        repo.insert(&original).unwrap();

        let replacement = Issuer::builder()
            .id(id)
            .name(IssuerName::new("HWUPKR0MPOU8FGXBT394").unwrap())
            .status(IssuerStatus::Retired)
            .build()
            .unwrap();
        assert_eq!(repo.update(&replacement, 1).unwrap(), WriteOutcome::Applied);

        let found = repo.find_by_id(&id, LoadMode::Lazy).unwrap().unwrap();
        assert_eq!(found.version, 2, "version should bump");
        assert_eq!(found.data.name().unwrap().as_str(), "HWUPKR0MPOU8FGXBT394");
        assert!(found.data.status().is_retired());
        assert!(
            found.data.lei().is_none(),
            "unset lei must be cleared to NULL (full replace, not patch)"
        );
    }

    #[test]
    fn update_preserves_id_and_created_at() {
        use chrono::SubsecRound;

        let (_db, repo) = test_repo();
        let id = IssuerId::new();
        let original = Issuer::builder().id(id).build().unwrap();
        let created_at = original.created_at().trunc_subsecs(3);
        repo.insert(&original).unwrap();

        let replacement = Issuer::builder()
            .id(id)
            .status(IssuerStatus::Retired)
            .build()
            .unwrap();
        assert_eq!(repo.update(&replacement, 1).unwrap(), WriteOutcome::Applied);

        let found = repo.find_by_id(&id, LoadMode::Lazy).unwrap().unwrap();
        assert_eq!(found.data.id(), &id);
        assert_eq!(
            found.data.created_at(),
            created_at,
            "created_at is immutable and must survive a full replace"
        );
    }

    #[test]
    fn update_with_stale_version_reports_version_mismatch() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let outcome = repo.update(&issuer, 99).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            },
            "stale-version update should report a version mismatch"
        );
    }

    #[test]
    fn update_on_missing_id_reports_missing() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        let outcome = repo.update(&issuer, 1).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn update_unique_collision_is_a_storage_fault() {
        let (_db, repo) = test_repo();

        let a = Issuer::builder()
            .lei(Lei::new("5493000IBP32UQZ0KL24").unwrap())
            .build()
            .unwrap();
        let b_id = IssuerId::new();
        let b = Issuer::builder()
            .id(b_id)
            .lei(Lei::new("213800WSGIIZCXF1P572").unwrap())
            .build()
            .unwrap();
        repo.insert(&a).unwrap();
        repo.insert(&b).unwrap();

        let clash = Issuer::builder()
            .id(b_id)
            .lei(Lei::new("5493000IBP32UQZ0KL24").unwrap())
            .build()
            .unwrap();
        let result = repo.update(&clash, 1);
        assert!(
            result.is_err(),
            "a UNIQUE collision must surface as a storage fault"
        );
    }

    #[test]
    fn insert_reconstituted_issuer_with_cnpj_and_non_br_country_is_a_storage_fault() {
        let (_db, repo) = test_repo();
        let id = IssuerId::new();

        let issuer = Issuer::reconstitute(IssuerSnapshot {
            id,
            status: IssuerStatus::Active,
            created_at: Utc::now(),
            name: Some(IssuerName::new("Foreign CNPJ Corp").unwrap()),
            cnpj: Some(Cnpj::new("12345678000195").unwrap()),
            lei: None,
            country_code: Some(CountryCode::from_str("US").unwrap()),
            securities: Loading::NotLoaded,
        });

        assert!(
            repo.insert(&issuer).is_err(),
            "a CHECK violation must surface as a storage fault"
        );
    }

    #[test]
    fn apply_patch_setting_non_br_country_on_issuer_with_cnpj_is_a_storage_fault() {
        let (_db, repo) = test_repo();

        let issuer = Issuer::builder()
            .cnpj(Cnpj::new("12345678000195").unwrap())
            .country_code(CountryCode::from_str("BR").unwrap())
            .build()
            .unwrap();

        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder()
            .country_code(CountryCode::from_str("US").unwrap())
            .build();

        assert!(
            repo.apply_patch(issuer.id(), 1, patch).is_err(),
            "a CHECK violation must surface as a storage fault"
        );
    }

    fn insert_sorted_ids(repo: &SqliteIssuerRepository, n: usize) -> Vec<IssuerId> {
        let mut ids: Vec<IssuerId> = Vec::with_capacity(n);
        for _ in 0..n {
            let issuer = Issuer::builder().build().unwrap();
            repo.insert(&issuer).unwrap();
            ids.push(*issuer.id());
        }
        ids.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        ids
    }

    #[test]
    fn insert_stores_created_at_in_canonical_utc_form() {
        use chrono::{SubsecRound, TimeZone, Utc};
        let (db, repo) = test_repo();
        let ts = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
            + chrono::Duration::microseconds(123_456);
        let issuer = Issuer::builder().created_at(ts).build().unwrap();
        repo.insert(&issuer).unwrap();

        let handle = db.handle();
        let raw: String = {
            let conn = handle.read();
            conn.query_row(
                "SELECT created_at FROM issuer WHERE id = ?1",
                rusqlite::params![issuer.id().as_bytes()],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(
            raw.ends_with('Z'),
            "stored timestamp must be Z-suffixed, got {raw:?}"
        );
        assert_eq!(
            raw, "2026-01-02T03:04:05.123Z",
            "stored timestamp must be canonical millisecond UTC"
        );

        let found = repo
            .find_by_id(issuer.id(), LoadMode::Lazy)
            .unwrap()
            .unwrap();
        assert_eq!(found.data.created_at(), ts.trunc_subsecs(3));
    }

    #[test]
    fn legacy_created_at_offset_format_still_reads() {
        let (db, repo) = test_repo();
        let id = IssuerId::new();

        {
            let handle = db.handle();
            let conn = handle.write();
            conn.execute(
                "INSERT INTO issuer (id, status, created_at) VALUES (?1, 'ACTIVE', ?2)",
                rusqlite::params![id.as_bytes(), "2026-01-02T03:04:05+00:00"],
            )
            .unwrap();
        }

        let found = repo
            .find_by_id(&id, LoadMode::Lazy)
            .unwrap()
            .expect("legacy row must parse");
        assert_eq!(
            found.data.created_at().to_rfc3339(),
            "2026-01-02T03:04:05+00:00"
        );
    }

    #[test]
    fn list_paged_on_empty_table_is_empty() {
        let (_db, repo) = test_repo();
        let page = repo.list_paged(None, 10, LoadMode::Lazy).unwrap();
        assert!(page.is_empty());
    }

    #[test]
    fn list_paged_first_page_respects_limit_and_order() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 5);

        let page = repo.list_paged(None, 2, LoadMode::Lazy).unwrap();
        let page_ids: Vec<IssuerId> = page.iter().map(|v| *v.data.id()).collect();

        assert_eq!(
            page_ids,
            ids[0..2],
            "first page must be the two smallest ids in order"
        );
    }

    #[test]
    fn list_paged_walks_all_pages_via_keyset_without_gaps_or_dupes() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 5);

        let mut collected: Vec<IssuerId> = Vec::new();
        let mut after: Option<IssuerId> = None;
        loop {
            let page = repo.list_paged(after, 2, LoadMode::Lazy).unwrap();
            if page.is_empty() {
                break;
            }
            for v in &page {
                collected.push(*v.data.id());
            }
            after = collected.last().copied();
        }

        assert_eq!(
            collected, ids,
            "keyset walk must yield every id exactly once, in order"
        );
    }

    #[test]
    fn list_paged_after_last_id_is_empty() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 3);

        let page = repo
            .list_paged(ids.last().copied(), 10, LoadMode::Lazy)
            .unwrap();
        assert!(page.is_empty(), "seeking past the last id yields no rows");
    }

    #[test]
    fn list_paged_middle_seek_returns_strictly_after() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 5);

        let page = repo.list_paged(Some(ids[1]), 2, LoadMode::Lazy).unwrap();
        let page_ids: Vec<IssuerId> = page.iter().map(|v| *v.data.id()).collect();
        assert_eq!(page_ids, ids[2..4]);
    }

    #[test]
    fn exists_by_cnpj_and_lei_report_stored_identifiers() {
        let (_db, repo) = test_repo();
        let cnpj = Cnpj::new("12345678000195").unwrap();
        let lei = Lei::new("5493000IBP32UQZ0KL24").unwrap();

        let other_lei = Lei::new("213800WSGIIZCXF1P572").unwrap();
        assert!(!repo.exists_by_cnpj(&cnpj).unwrap());
        assert!(!repo.exists_by_lei(&lei).unwrap());

        let issuer = Issuer::builder()
            .cnpj(cnpj.clone())
            .lei(lei.clone())
            .build()
            .unwrap();
        repo.insert(&issuer).unwrap();

        assert!(repo.exists_by_cnpj(&cnpj).unwrap());
        assert!(repo.exists_by_lei(&lei).unwrap());
        assert!(matches!(repo.exists_by_lei(&other_lei), Ok(false)));
    }

    // --- eager/lazy loading -------------------------------------------------

    /// Issuer + N securities inserted through the repositories.
    fn seed_issuer_with_securities(
        db: &Database,
        repo: &SqliteIssuerRepository,
        n: usize,
    ) -> (IssuerId, Vec<SecurityId>) {
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let securities = SqliteSecurityRepository::new(db.handle());
        let mut ids = Vec::with_capacity(n);
        for _ in 0..n {
            let security = Security::builder(*issuer.id(), SecurityKind::CommonShare)
                .build()
                .unwrap();
            securities.insert(&security).unwrap();
            ids.push(*security.id());
        }
        ids.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        (*issuer.id(), ids)
    }

    #[test]
    fn find_by_id_lazy_leaves_securities_not_loaded() {
        let (db, repo) = test_repo();
        let (issuer_id, _) = seed_issuer_with_securities(&db, &repo, 2);

        let found = repo
            .find_by_id(&issuer_id, LoadMode::Lazy)
            .unwrap()
            .unwrap();
        assert!(
            found.data.securities().is_none(),
            "lazy reads must not fetch securities"
        );
    }

    #[test]
    fn find_by_id_eager_loads_exactly_the_issuers_securities_in_id_order() {
        let (db, repo) = test_repo();
        let (issuer_id, security_ids) = seed_issuer_with_securities(&db, &repo, 3);
        // Another issuer's securities must not bleed in.
        let (_other, _) = seed_issuer_with_securities(&db, &repo, 2);

        let found = repo
            .find_by_id(&issuer_id, LoadMode::Eager)
            .unwrap()
            .unwrap();
        let loaded = found.data.securities().expect("eager read must hydrate");
        let loaded_ids: Vec<SecurityId> = loaded.iter().map(|s| *s.id()).collect();

        assert_eq!(loaded_ids, security_ids);
        assert!(loaded.iter().all(|s| s.issuer_id() == &issuer_id));
    }

    #[test]
    fn find_by_id_eager_on_issuer_without_securities_is_loaded_empty() {
        let (db, repo) = test_repo();
        let (issuer_id, _) = seed_issuer_with_securities(&db, &repo, 0);

        let found = repo
            .find_by_id(&issuer_id, LoadMode::Eager)
            .unwrap()
            .unwrap();
        assert!(
            matches!(found.data.securities(), Some(s) if s.is_empty()),
            "eager read of a childless issuer must be Loaded(empty), not NotLoaded"
        );
    }

    #[test]
    fn list_all_eager_groups_securities_per_issuer_without_cross_contamination() {
        let (db, repo) = test_repo();
        let (issuer_a, ids_a) = seed_issuer_with_securities(&db, &repo, 2);
        let (issuer_b, ids_b) = seed_issuer_with_securities(&db, &repo, 3);
        let (issuer_c, _) = seed_issuer_with_securities(&db, &repo, 0);

        let listed = repo.list_all(LoadMode::Eager).unwrap();
        assert_eq!(listed.len(), 3);

        for versioned in &listed {
            let issuer = &versioned.data;
            let loaded = issuer.securities().expect("eager list must hydrate all");
            let loaded_ids: Vec<SecurityId> = loaded.iter().map(|s| *s.id()).collect();

            if issuer.id() == &issuer_a {
                assert_eq!(loaded_ids, ids_a);
            } else if issuer.id() == &issuer_b {
                assert_eq!(loaded_ids, ids_b);
            } else if issuer.id() == &issuer_c {
                assert!(loaded_ids.is_empty());
            } else {
                panic!("unexpected issuer in listing");
            }
            assert!(loaded.iter().all(|s| s.issuer_id() == issuer.id()));
        }
    }

    #[test]
    fn list_all_lazy_leaves_every_issuer_not_loaded() {
        let (db, repo) = test_repo();
        seed_issuer_with_securities(&db, &repo, 2);
        seed_issuer_with_securities(&db, &repo, 1);

        let listed = repo.list_all(LoadMode::Lazy).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|v| v.data.securities().is_none()));
    }

    #[test]
    fn list_paged_eager_hydrates_exactly_the_page() {
        let (db, repo) = test_repo();
        let mut seeded: Vec<(IssuerId, Vec<SecurityId>)> = (0..5)
            .map(|n| seed_issuer_with_securities(&db, &repo, n % 3))
            .collect();
        seeded.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));

        let mut after: Option<IssuerId> = None;
        let mut walked = 0usize;
        loop {
            let page = repo.list_paged(after, 2, LoadMode::Eager).unwrap();
            if page.is_empty() {
                break;
            }
            for versioned in &page {
                let issuer = &versioned.data;
                let expected = seeded
                    .iter()
                    .find(|(id, _)| id == issuer.id())
                    .map(|(_, ids)| ids.clone())
                    .expect("page must only contain seeded issuers");
                let loaded_ids: Vec<SecurityId> = issuer
                    .securities()
                    .expect("eager page must hydrate")
                    .iter()
                    .map(|s| *s.id())
                    .collect();
                assert_eq!(loaded_ids, expected);
                walked += 1;
            }
            after = page.last().map(|v| *v.data.id());
        }

        assert_eq!(walked, seeded.len(), "keyset walk must cover every issuer");
    }

    #[test]
    fn eager_loaded_securities_carry_full_state() {
        let (db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let securities = SqliteSecurityRepository::new(db.handle());
        let security = Security::builder(*issuer.id(), SecurityKind::PreferredShare)
            .name(SecurityName::new("Acme PN").unwrap())
            .isin(Isin::new("BRVALEACNOR0").unwrap())
            .status(SecurityStatus::Active)
            .build()
            .unwrap();
        securities.insert(&security).unwrap();

        let found = repo
            .find_by_id(issuer.id(), LoadMode::Eager)
            .unwrap()
            .unwrap();
        let loaded = found.data.securities().unwrap();
        assert_eq!(loaded.len(), 1);
        let child = loaded.first().unwrap();
        assert_eq!(child.id(), security.id());
        assert_eq!(child.kind(), SecurityKind::PreferredShare);
        assert_eq!(child.name().unwrap().as_str(), "Acme PN");
        assert_eq!(child.isin(), security.isin());
    }
}

#[cfg(test)]
mod tests_models {
    use super::*;
    use crate::sqlite::database::{Database, Db};
    use std::str::FromStr;
    use valqeron_core::domain::issuer::{IssuerId, IssuerStatus};

    #[test]
    fn issuer_row_round_trips_all_columns() {
        let db = Database::open_temp();
        let handle = db.handle();

        let id = IssuerId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO issuer (id, name, status, created_at, cnpj, lei, country_code, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    id.as_bytes(),
                    "Acme Corp",
                    "RETIRED",
                    "2026-01-02T03:04:05+00:00",
                    "12.345.678/0001-95",
                    Option::<String>::None,
                    "BR",
                    7u32,
                ],
            )
                .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, name, status, created_at, cnpj, lei, country_code, version
                 FROM issuer WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                IssuerRow::from_row,
            )
            .unwrap();

        let Versioned { data, version } = row.into_inner();
        assert_eq!(data.id, id);
        assert_eq!(data.status, IssuerStatus::Retired);
        assert_eq!(data.created_at.to_rfc3339(), "2026-01-02T03:04:05+00:00");
        assert_eq!(data.name.unwrap().as_str(), "Acme Corp");
        assert_eq!(data.cnpj.unwrap(), Cnpj::new("12.345.678/0001-95").unwrap());
        assert!(data.lei.is_none());
        assert_eq!(
            data.country_code.unwrap(),
            CountryCode::from_str("BR").unwrap()
        );
        assert!(
            matches!(data.securities, Loading::NotLoaded),
            "row mapping alone must not claim the securities relation was fetched"
        );
        assert_eq!(version, 7);
    }

    #[test]
    fn issuer_row_maps_null_optionals_to_none() {
        let db = Database::open_temp();
        let handle = db.handle();

        let id = IssuerId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO issuer (id, status, created_at) VALUES (?1, 'ACTIVE', ?2)",
                rusqlite::params![id.as_bytes(), "2026-01-01T00:00:00+00:00"],
            )
            .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, name, status, created_at, cnpj, lei, country_code, version
                 FROM issuer WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                IssuerRow::from_row,
            )
            .unwrap();

        let snapshot = row.into_inner().data;
        assert!(snapshot.name.is_none());
        assert!(snapshot.cnpj.is_none());
        assert!(snapshot.lei.is_none());
        assert!(snapshot.country_code.is_none());
        assert_eq!(snapshot.status, IssuerStatus::Active);
    }

    #[test]
    fn issuer_row_rejects_invalid_status() {
        let db = Database::open_temp();
        let handle = db.handle();

        let conn = handle.read();
        let result: rusqlite::Result<IssuerRow> = conn.query_row(
            "SELECT randomblob(16) AS id, NULL AS name, 'BOGUS' AS status,
                    '2026-01-01T00:00:00+00:00' AS created_at, NULL AS cnpj,
                    NULL AS lei, NULL AS country_code, 1 AS version",
            [],
            IssuerRow::from_row,
        );

        assert!(matches!(
            result,
            Err(rusqlite::Error::FromSqlConversionFailure(..))
        ));
    }
}

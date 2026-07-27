use crate::common::{CnpjIdentifier, LeiIdentifier, Versioned};
use crate::db::{SharedConnection, lock};
use crate::issuer::error::IssuerRepositoryError;
use crate::issuer::patch::IssuerPatch;
use crate::issuer::repository::IssuerRepository;
use crate::issuer::{Issuer, IssuerId, IssuerName, IssuerStatus};
use chrono::{DateTime, Utc};
use ftracker_identifiers::CountryCode;
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;

pub struct SqliteIssuerRepository {
    conn: SharedConnection,
}

impl SqliteIssuerRepository {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }
}

impl IssuerRepository for SqliteIssuerRepository {
    fn find_by_id(
        &self,
        id: &IssuerId,
    ) -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, status, created_at, cnpj, lei, country_code, version
                 FROM issuer WHERE id = ?1",
            )
            .map_err(infra)?;

        stmt.query_row(params![id.as_bytes()], row_to_versioned_issuer)
            .optional()
            .map_err(infra)
    }

    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")
            .map_err(infra)?;

        stmt.query_row(params![id.as_bytes()], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
            .map_err(infra)
    }

    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO issuer (id, name, status, created_at, cnpj, lei, country_code)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .map_err(infra)?;

        let result = stmt.execute(params![
            issuer.id().as_bytes(),
            issuer.name().map(IssuerName::as_str),
            status_as_str(issuer.status()),
            issuer.created_at().to_rfc3339(),
            issuer.cnpj().map(|c| c.as_str()),
            issuer.lei().map(|l| l.as_str()),
            issuer.country_code().map(|c| c.as_str()),
        ]);

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(IssuerRepositoryError::Conflict(format!(
                "issuer {} already exists (id or unique CNPJ/LEI collision)",
                issuer.id().value()
            ))),
            Err(e) => Err(infra(e)),
        }
    }

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare_cached(
                "UPDATE issuer SET
                name = COALESCE(?2, name),
                status = COALESCE(?3, status),
                cnpj = COALESCE(?4, cnpj),
                lei = COALESCE(?5, lei),
                country_code = COALESCE(?6, country_code),
                version = version + 1
             WHERE id = ?1 AND version = ?7",
            )
            .map_err(infra)?;

        let result = stmt.execute(params![
            id.as_bytes(),
            patch.name.as_ref().map(IssuerName::as_str),
            patch.status.map(status_as_str),
            patch.cnpj.as_ref().map(|c| c.as_str()),
            patch.lei.as_ref().map(|l| l.as_str()),
            patch.country_code.as_ref().map(|c| c.as_str()),
            expected_version
        ]);

        match result {
            Ok(0) => {
                let mut check_stmt = conn
                    .prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")
                    .map_err(infra)?;

                let id_exists = check_stmt
                    .query_row(params![id.as_bytes()], |_| Ok(()))
                    .optional()
                    .map_err(infra)?
                    .is_some();

                if id_exists {
                    Err(IssuerRepositoryError::Conflict(format!(
                        "version mismatch: issuer {} was modified by another process",
                        id.value()
                    )))
                } else {
                    Err(not_found(id))
                }
            }
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(IssuerRepositoryError::Conflict(format!(
                "patch on issuer {} would violate a unique constraint (CNPJ/LEI already in use)",
                id.value()
            ))),
            Err(e) => Err(infra(e)),
        }
    }

    fn delete(&self, id: &IssuerId) -> Result<(), IssuerRepositoryError> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare_cached("DELETE FROM issuer WHERE id = ?1")
            .map_err(infra)?;

        let rows = stmt.execute(params![id.as_bytes()]).map_err(infra)?;

        if rows == 0 {
            return Err(not_found(id));
        }
        Ok(())
    }
}

fn infra(e: impl Into<anyhow::Error>) -> IssuerRepositoryError {
    IssuerRepositoryError::Infrastructure(e.into())
}

fn not_found(id: &IssuerId) -> IssuerRepositoryError {
    IssuerRepositoryError::NotFound(*id)
}

fn is_constraint_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn status_as_str(status: IssuerStatus) -> String {
    status.into()
}

fn row_to_versioned_issuer(row: &Row) -> rusqlite::Result<Versioned<Issuer>> {
    let id_bytes: Vec<u8> = row.get("id")?;
    let id = IssuerId::from_uuid(uuid::Uuid::from_slice(&id_bytes).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(e))
    })?);

    let status_str: String = row.get("status")?;
    let status = IssuerStatus::from_str(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let created_at_str: String = row.get("created_at")?;
    let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let name: Option<String> = row.get("name")?;
    let name = name.map(IssuerName::new).transpose().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let cnpj: Option<String> = row.get("cnpj")?;
    let cnpj = cnpj.map(CnpjIdentifier::new);

    let lei: Option<String> = row.get("lei")?;
    let lei = lei.map(LeiIdentifier::new);

    let country_code: Option<String> = row.get("country_code")?;
    let country_code = country_code
        .map(|s| CountryCode::from_str(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let version: u32 = row.get("version")?;

    let issuer = Issuer::reconstitute(id, status, created_at, name, cnpj, lei, country_code);

    Ok(Versioned {
        data: issuer,
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_repo() -> SqliteIssuerRepository {
        let db = Database::open_in_memory().unwrap();
        SqliteIssuerRepository::new(db.connection())
    }

    #[test]
    fn insert_then_find_round_trips_and_defaults_to_version_1() {
        let repo = test_repo();
        let issuer = Issuer::builder()
            .name(IssuerName::new("Acme Corp").unwrap())
            .build()
            .unwrap();

        repo.insert(&issuer).unwrap();

        let found = repo
            .find_by_id(issuer.id())
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
        let repo = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        // Patch with expected version 1
        repo.apply_patch(issuer.id(), 1, patch).unwrap();

        let updated = repo.find_by_id(issuer.id()).unwrap().unwrap();
        assert!(updated.data.status().is_retired());
        assert_eq!(updated.version, 2, "Version should be incremented to 2");
    }

    #[test]
    fn apply_patch_fails_on_stale_version() {
        let repo = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        // Attempt to patch expecting version 99 (stale/incorrect)
        let result = repo.apply_patch(issuer.id(), 99, patch);

        assert!(
            matches!(result, Err(IssuerRepositoryError::Conflict(msg)) if msg.contains("version mismatch")),
            "Expected a Conflict error due to version mismatch"
        );
    }

    #[test]
    fn apply_patch_on_missing_id_returns_not_found() {
        let repo = test_repo();
        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        let result = repo.apply_patch(&IssuerId::new(), 1, patch);
        assert!(matches!(result, Err(IssuerRepositoryError::NotFound(_))));
    }

    #[test]
    fn insert_duplicate_id_is_a_conflict_not_an_infra_error() {
        let repo = test_repo();
        let issuer = Issuer::builder().build().unwrap();

        repo.insert(&issuer).unwrap();
        let result = repo.insert(&issuer);

        assert!(matches!(result, Err(IssuerRepositoryError::Conflict(_))));
    }
}

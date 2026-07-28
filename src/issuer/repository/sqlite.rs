use crate::common::{CnpjIdentifier, LeiIdentifier, Versioned};
use crate::db::DbHandle;
use crate::issuer::error::IssuerRepositoryError;
use crate::issuer::patch::IssuerPatch;
use crate::issuer::repository::IssuerRepository;
use crate::issuer::{Issuer, IssuerId, IssuerName, IssuerStatus};
use chrono::{DateTime, Utc};
use ftracker_identifiers::CountryCode;
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;

/// [`IssuerRepository`] backed by SQLite. Reads using the connection pool; writes using the serialized writer.
pub struct SqliteIssuerRepository {
    db: DbHandle,
}

impl SqliteIssuerRepository {
    /// Create a repository over the given database handle.
    pub fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

impl IssuerRepository for SqliteIssuerRepository {
    fn find_by_id(
        &self,
        id: &IssuerId,
    ) -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError> {
        let conn = self.db.read();
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
        let conn = self.db.read();
        let mut stmt = conn
            .prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")
            .map_err(infra)?;

        stmt.query_row(params![id.as_bytes()], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
            .map_err(infra)
    }

    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError> {
        let conn = self.db.write();
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
            Err(e) if is_constraint_violation(&e) => Err(handle_constraint_error(
                &e,
                &format!("insert on issuer {}", issuer.id().value()),
            )),
            Err(e) => Err(infra(e)),
        }
    }

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError> {
        let conn = self.db.write();
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
            Ok(0) => Err(conflict_or_not_found(&conn, id)),
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(handle_constraint_error(
                &e,
                &format!("patch on issuer {}", id.value()),
            )),
            Err(e) => Err(infra(e)),
        }
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        let id = issuer.id();
        let conn = self.db.write();

        let mut stmt = conn
            .prepare_cached(
                "UPDATE issuer SET
                name = ?2,
                status = ?3,
                cnpj = ?4,
                lei = ?5,
                country_code = ?6,
                version = version + 1
             WHERE id = ?1 AND version = ?7",
            )
            .map_err(infra)?;

        let result = stmt.execute(params![
            id.as_bytes(),
            issuer.name().map(IssuerName::as_str),
            status_as_str(issuer.status()),
            issuer.cnpj().map(|c| c.as_str()),
            issuer.lei().map(|l| l.as_str()),
            issuer.country_code().map(|c| c.as_str()),
            expected_version
        ]);

        match result {
            Ok(0) => Err(conflict_or_not_found(&conn, id)),
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(handle_constraint_error(
                &e,
                &format!("update on issuer {}", id.value()),
            )),
            Err(e) => Err(infra(e)),
        }
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        let conn = self.db.write();
        let mut stmt = conn
            .prepare_cached("DELETE FROM issuer WHERE id = ?1 AND version = ?2")
            .map_err(infra)?;

        let rows = stmt
            .execute(params![id.as_bytes(), expected_version])
            .map_err(infra)?;

        if rows == 0 {
            return Err(conflict_or_not_found(&conn, id));
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

/// After a versioned write affects 0 rows, decide whether the row exists with a different version
/// (`Conflict`) or is absent (`NotFound`). Must be called while still holding the writer lock so
/// the check is race-free.
fn conflict_or_not_found(conn: &rusqlite::Connection, id: &IssuerId) -> IssuerRepositoryError {
    let exists = conn
        .prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")
        .and_then(|mut stmt| {
            stmt.query_row(params![id.as_bytes()], |_| Ok(()))
                .optional()
        });

    match exists {
        Ok(Some(_)) => IssuerRepositoryError::Conflict(format!(
            "version mismatch: issuer {} was modified by another process",
            id.value()
        )),
        Ok(None) => not_found(id),
        Err(e) => infra(e),
    }
}

fn is_constraint_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn handle_constraint_error(err: &rusqlite::Error, entity_desc: &str) -> IssuerRepositoryError {
    let default_msg = format!("{} violated a database constraint", entity_desc);

    let msg = if let rusqlite::Error::SqliteFailure(_, Some(sqlite_msg)) = err {
        if sqlite_msg.contains("CHECK") {
            format!(
                "{} violated a CHECK constraint (e.g., CNPJ requires country_code 'BR')",
                entity_desc
            )
        } else if sqlite_msg.contains("UNIQUE") {
            format!(
                "{} violated a UNIQUE constraint (e.g., id or CNPJ/LEI collision)",
                entity_desc
            )
        } else {
            default_msg
        }
    } else {
        default_msg
    };

    IssuerRepositoryError::Conflict(msg)
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

    fn test_repo() -> (Database, SqliteIssuerRepository) {
        let db = Database::open_in_memory().unwrap();
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
        let (_db, repo) = test_repo();
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
        let (_db, repo) = test_repo();
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
        let (_db, repo) = test_repo();
        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        let result = repo.apply_patch(&IssuerId::new(), 1, patch);
        assert!(matches!(result, Err(IssuerRepositoryError::NotFound(_))));
    }

    #[test]
    fn insert_duplicate_id_is_a_conflict_not_an_infra_error() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();

        repo.insert(&issuer).unwrap();
        let result = repo.insert(&issuer);

        assert!(matches!(result, Err(IssuerRepositoryError::Conflict(_))));
    }

    #[test]
    fn delete_with_correct_version_removes_the_row() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        repo.delete(issuer.id(), 1).unwrap();
        assert!(repo.find_by_id(issuer.id()).unwrap().is_none());
    }

    #[test]
    fn delete_with_stale_version_is_a_conflict() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let result = repo.delete(issuer.id(), 99);
        assert!(
            matches!(result, Err(IssuerRepositoryError::Conflict(msg)) if msg.contains("version mismatch")),
            "stale-version delete should be a Conflict, not a silent no-op"
        );
        // Row must still be present.
        assert!(repo.find_by_id(issuer.id()).unwrap().is_some());
    }

    #[test]
    fn delete_missing_id_returns_not_found() {
        let (_db, repo) = test_repo();
        let result = repo.delete(&IssuerId::new(), 1);
        assert!(matches!(result, Err(IssuerRepositoryError::NotFound(_))));
    }

    #[test]
    fn dry_run_repository_writes_are_rolled_back() {
        let db = Database::open_in_memory().unwrap();
        let issuer = Issuer::builder().build().unwrap();

        db.dry_run(|h| {
            let repo = SqliteIssuerRepository::new(h.clone());
            repo.insert(&issuer).unwrap();
            assert!(repo.find_by_id(issuer.id()).unwrap().is_some());
        })
        .unwrap();

        let repo = SqliteIssuerRepository::new(db.handle());
        assert!(
            repo.find_by_id(issuer.id()).unwrap().is_none(),
            "dry-run insert must not persist"
        );
    }

    #[test]
    fn update_replaces_all_fields_and_clears_optionals_to_null() {
        let (_db, repo) = test_repo();
        let id = IssuerId::new();

        // Insert with name + lei set.
        let original = Issuer::builder()
            .id(id)
            .name(IssuerName::new("Acme Corp").unwrap())
            .lei(LeiIdentifier::new("LEI-ORIGINAL"))
            .build()
            .unwrap();
        repo.insert(&original).unwrap();

        // Full replace: rename and drop the lei entirely.
        let replacement = Issuer::builder()
            .id(id)
            .name(IssuerName::new("Renamed Corp").unwrap())
            .status(IssuerStatus::Retired)
            .build()
            .unwrap();
        repo.update(&replacement, 1).unwrap();

        let found = repo.find_by_id(&id).unwrap().unwrap();
        assert_eq!(found.version, 2, "version should bump");
        assert_eq!(found.data.name().unwrap().as_str(), "Renamed Corp");
        assert!(found.data.status().is_retired());
        assert!(
            found.data.lei().is_none(),
            "unset lei must be cleared to NULL (full replace, not patch)"
        );
    }

    #[test]
    fn update_preserves_id_and_created_at() {
        let (_db, repo) = test_repo();
        let id = IssuerId::new();
        let original = Issuer::builder().id(id).build().unwrap();
        let created_at = original.created_at();
        repo.insert(&original).unwrap();

        let replacement = Issuer::builder()
            .id(id)
            .status(IssuerStatus::Retired)
            .build()
            .unwrap();
        repo.update(&replacement, 1).unwrap();

        let found = repo.find_by_id(&id).unwrap().unwrap();
        assert_eq!(found.data.id(), &id);
        assert_eq!(
            found.data.created_at(),
            created_at,
            "created_at is immutable and must survive a full replace"
        );
    }

    #[test]
    fn update_with_stale_version_is_a_conflict() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        repo.insert(&issuer).unwrap();

        let result = repo.update(&issuer, 99);
        assert!(
            matches!(result, Err(IssuerRepositoryError::Conflict(msg)) if msg.contains("version mismatch")),
            "stale-version update should be a Conflict"
        );
    }

    #[test]
    fn update_on_missing_id_returns_not_found() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        let result = repo.update(&issuer, 1);
        assert!(matches!(result, Err(IssuerRepositoryError::NotFound(_))));
    }

    #[test]
    fn update_unique_collision_is_a_conflict() {
        let (_db, repo) = test_repo();

        let a = Issuer::builder()
            .lei(LeiIdentifier::new("LEI-A"))
            .build()
            .unwrap();
        let b_id = IssuerId::new();
        let b = Issuer::builder()
            .id(b_id)
            .lei(LeiIdentifier::new("LEI-B"))
            .build()
            .unwrap();
        repo.insert(&a).unwrap();
        repo.insert(&b).unwrap();

        // Try to update b's lei to a's lei → UNIQUE violation.
        let clash = Issuer::builder()
            .id(b_id)
            .lei(LeiIdentifier::new("LEI-A"))
            .build()
            .unwrap();
        let result = repo.update(&clash, 1);
        assert!(matches!(result, Err(IssuerRepositoryError::Conflict(_))));
    }

    #[test]
    fn insert_reconstituted_issuer_with_cnpj_and_non_br_country_fails() {
        let (_db, repo) = test_repo();
        let id = IssuerId::new();

        let issuer = Issuer::reconstitute(
            id,
            IssuerStatus::Active,
            Utc::now(),
            Some(IssuerName::new("Foreign CNPJ Corp").unwrap()),
            Some(CnpjIdentifier::new("12345678000195")),
            None,
            Some(CountryCode::from_str("US").unwrap()),
        );

        let result = repo.insert(&issuer);

        assert!(
            matches!(result, Err(IssuerRepositoryError::Conflict(ref msg)) if msg.contains("CHECK constraint")),
            "Expected CHECK constraint violation for CNPJ with US country code, got: {:?}",
            result
        );
    }

    #[test]
    fn apply_patch_setting_non_br_country_on_issuer_with_cnpj_fails() {
        let (_db, repo) = test_repo();

        let issuer = Issuer::builder()
            .cnpj(CnpjIdentifier::new("12345678000195"))
            .country_code(CountryCode::from_str("BR").unwrap())
            .build()
            .unwrap();

        repo.insert(&issuer).unwrap();

        let patch = IssuerPatch::builder()
            .country_code(CountryCode::from_str("US").unwrap())
            .build();

        let result = repo.apply_patch(issuer.id(), 1, patch);

        assert!(
            matches!(result, Err(IssuerRepositoryError::Conflict(ref msg)) if msg.contains("CHECK constraint")),
            "Expected CHECK constraint violation when patching country to US on an issuer holding a CNPJ, got: {:?}",
            result
        );
    }
}

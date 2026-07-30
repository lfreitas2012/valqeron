use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::{
    Issuer, IssuerId, IssuerPatch, IssuerRepository, RepositoryError, RepositoryResult, Versioned,
};

use crate::sqlite::driver::DbHandle;
use crate::sqlite::queries;

/// [`IssuerRepository`] backed by SQLite. Reads use the connection pool; writes use the serialized writer.
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
    fn find_by_id(&self, id: &IssuerId) -> RepositoryResult<Option<Versioned<Issuer>>> {
        let conn = self.db.read();
        queries::find_by_id(&conn, id)
            .map(|opt| opt.map(|row| row.into_inner()))
            .map_err(backend)
    }

    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Issuer>>> {
        let conn = self.db.read();

        queries::list_all(&conn)
            .map(|rows| rows.into_iter().map(|row| row.into_inner()).collect())
            .map_err(backend)
    }

    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists(&conn, id).map_err(backend)
    }

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
        let conn = self.db.write();
        match queries::insert(&conn, issuer) {
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(constraint_conflict(
                &e,
                &format!("insert on issuer {}", issuer.id().value()),
            )),
            Err(e) => Err(backend(e)),
        }
    }

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<()> {
        let conn = self.db.write();
        match queries::apply_patch(&conn, id, expected_version, &patch) {
            Ok(0) => Err(conflict_or_not_found(&conn, id)),
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(constraint_conflict(
                &e,
                &format!("patch on issuer {}", id.value()),
            )),
            Err(e) => Err(backend(e)),
        }
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<()> {
        let id = issuer.id();
        let conn = self.db.write();
        match queries::update(&conn, issuer, expected_version) {
            Ok(0) => Err(conflict_or_not_found(&conn, id)),
            Ok(_) => Ok(()),
            Err(e) if is_constraint_violation(&e) => Err(constraint_conflict(
                &e,
                &format!("update on issuer {}", id.value()),
            )),
            Err(e) => Err(backend(e)),
        }
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<()> {
        let conn = self.db.write();
        match queries::delete(&conn, id, expected_version) {
            Ok(0) => Err(conflict_or_not_found(&conn, id)),
            Ok(_) => Ok(()),
            Err(e) => Err(backend(e)),
        }
    }
}

/// Wrap a raw driver error as a driver-agnostic [`RepositoryError::Backend`].
fn backend(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Backend(anyhow::Error::new(e))
}

/// After a versioned write affects 0 rows, decide whether the row exists with a different version
/// (`Conflict`) or is absent (`NotFound`). Must be called while still holding the writer lock so
/// the check is race-free.
fn conflict_or_not_found(conn: &Connection, id: &IssuerId) -> RepositoryError {
    let exists = conn
        .prepare_cached("SELECT 1 FROM issuer WHERE id = ?1")
        .and_then(|mut stmt| {
            stmt.query_row(params![id.as_bytes()], |_| Ok(()))
                .optional()
        });

    match exists {
        Ok(Some(_)) => RepositoryError::Conflict(format!(
            "version mismatch: issuer {} was modified by another process",
            id.value()
        )),
        Ok(None) => RepositoryError::NotFound(*id),
        Err(e) => backend(e),
    }
}

fn is_constraint_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn constraint_conflict(err: &rusqlite::Error, entity_desc: &str) -> RepositoryError {
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

    RepositoryError::Conflict(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::driver::Database;
    use chrono::Utc;
    use ftracker_identifiers::{Cnpj, CountryCode, Lei};
    use std::str::FromStr;
    use valqeron_core::{IssuerName, IssuerStatus};

    fn test_repo() -> (Database, SqliteIssuerRepository) {
        let db = Database::open_in_memory().unwrap();
        let repo = SqliteIssuerRepository::new(db.handle());
        (db, repo)
    }

    /// Extract the typed [`RepositoryError`] from a failing repository result.
    fn expect_err<T: std::fmt::Debug>(result: RepositoryResult<T>) -> RepositoryError {
        result.expect_err("expected an error")
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

        let result = repo.apply_patch(issuer.id(), 99, patch);

        assert!(
            matches!(expect_err(result), RepositoryError::Conflict(msg) if msg.contains("version mismatch")),
            "Expected a Conflict error due to version mismatch"
        );
    }

    #[test]
    fn apply_patch_on_missing_id_returns_not_found() {
        let (_db, repo) = test_repo();
        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();

        let result = repo.apply_patch(&IssuerId::new(), 1, patch);
        assert!(matches!(expect_err(result), RepositoryError::NotFound(_)));
    }

    #[test]
    fn insert_duplicate_id_is_a_conflict_not_an_infra_error() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();

        repo.insert(&issuer).unwrap();
        let result = repo.insert(&issuer);

        assert!(matches!(expect_err(result), RepositoryError::Conflict(_)));
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
            matches!(expect_err(result), RepositoryError::Conflict(msg) if msg.contains("version mismatch")),
            "stale-version delete should be a Conflict, not a silent no-op"
        );
        assert!(repo.find_by_id(issuer.id()).unwrap().is_some());
    }

    #[test]
    fn delete_missing_id_returns_not_found() {
        let (_db, repo) = test_repo();
        let result = repo.delete(&IssuerId::new(), 1);
        assert!(matches!(expect_err(result), RepositoryError::NotFound(_)));
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
        repo.update(&replacement, 1).unwrap();

        let found = repo.find_by_id(&id).unwrap().unwrap();
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
            matches!(expect_err(result), RepositoryError::Conflict(msg) if msg.contains("version mismatch")),
            "stale-version update should be a Conflict"
        );
    }

    #[test]
    fn update_on_missing_id_returns_not_found() {
        let (_db, repo) = test_repo();
        let issuer = Issuer::builder().build().unwrap();
        let result = repo.update(&issuer, 1);
        assert!(matches!(expect_err(result), RepositoryError::NotFound(_)));
    }

    #[test]
    fn update_unique_collision_is_a_conflict() {
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
        assert!(matches!(expect_err(result), RepositoryError::Conflict(_)));
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
            Some(Cnpj::new("12345678000195").unwrap()),
            None,
            Some(CountryCode::from_str("US").unwrap()),
        );

        let err = expect_err(repo.insert(&issuer));

        assert!(
            matches!(&err, RepositoryError::Conflict(msg) if msg.contains("CHECK constraint")),
            "Expected CHECK constraint violation for CNPJ with US country code, got: {:?}",
            err
        );
    }

    #[test]
    fn apply_patch_setting_non_br_country_on_issuer_with_cnpj_fails() {
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

        let err = expect_err(repo.apply_patch(issuer.id(), 1, patch));

        assert!(
            matches!(&err, RepositoryError::Conflict(msg) if msg.contains("CHECK constraint")),
            "Expected CHECK constraint violation when patching country to US on an issuer holding a CNPJ, got: {:?}",
            err
        );
    }
}

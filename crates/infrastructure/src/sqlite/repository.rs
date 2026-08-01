use std::time::Duration;

use crate::sqlite::db::{Db, DbHandle};
use crate::sqlite::queries;
use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::{
    Issuer, IssuerId, IssuerPatch, IssuerRepository, RepositoryError, RepositoryResult, Versioned,
};

/// Maximum number of attempts a 'write' makes when SQLite reports the database as busy/locked.
const BUSY_MAX_ATTEMPTS: u32 = 5;

/// Base backoff between busy retries; grows linearly per attempt.
const BUSY_BACKOFF_BASE: Duration = Duration::from_millis(10);

/// [`IssuerRepository`] backed by SQLite. 'Reads' use the connection pool; 'writes' use the serialized writer.
///
/// Generic over the connection source `D`: the normal path uses [`DbHandle`] (the default), while
/// [`crate::sqlite::DatabaseConnection::dry_run`] instantiates it with a borrowed dry-run handle so the same
/// code runs inside a rolled-back savepoint without opening a second connection.
pub struct SqliteIssuerRepository<D: Db = DbHandle> {
    db: D,
}

impl<D: Db> SqliteIssuerRepository<D> {
    /// Create a repository over the given database handle.
    pub fn new(db: D) -> Self {
        Self { db }
    }
}

impl<D: Db> IssuerRepository for SqliteIssuerRepository<D> {
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

    fn list_paged(
        &self,
        after: Option<IssuerId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Issuer>>> {
        let conn = self.db.read();

        queries::list_paged(&conn, after.as_ref(), limit)
            .map(|rows| rows.into_iter().map(|row| row.into_inner()).collect())
            .map_err(backend)
    }

    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists(&conn, id).map_err(backend)
    }

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::insert(&conn, issuer) {
                Ok(_) => Ok(()),
                Err(e) if is_constraint_violation(&e) => Err(constraint_conflict(
                    &e,
                    &format!("insert on issuer {}", issuer.id().value()),
                )),
                Err(e) => Err(backend(e)),
            }
        })
    }

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<()> {
        with_busy_retry(|| {
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
        })
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<()> {
        let id = issuer.id();
        with_busy_retry(|| {
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
        })
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::delete(&conn, id, expected_version) {
                Ok(0) => Err(conflict_or_not_found(&conn, id)),
                Ok(_) => Ok(()),
                Err(e) => Err(backend(e)),
            }
        })
    }
}

/// Wrap a raw driver error as a driver-agnostic [`RepositoryError::DatabaseError`].
fn backend(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::DatabaseError(anyhow::Error::new(e))
}

/// Whether a rusqlite error is a transient busy/locked condition worth retrying.
///
/// `SQLITE_BUSY` can still surface after `busy_timeout` (e.g. cross-process writers), and
/// `SQLITE_LOCKED` is not covered by `busy_timeout` at all — both are transient and safe to retry
/// for an idempotent, self-contained write attempt.
fn is_busy_or_locked(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::DatabaseBusy
                || e.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// Run a write operation, retrying with a short linear backoff while SQLite reports the database as
/// busy/locked.
///
/// The operation closure must (re)acquire the writer guard on each attempt so the lock is not held
/// across the backoff sleep. Only raw busy/locked driver errors are retried; every other outcome —
/// success, `Conflict`, `NotFound`, constraint violations, or any other backend error — returns
/// immediately without retry.
fn with_busy_retry<T>(op: impl Fn() -> RepositoryResult<T>) -> RepositoryResult<T> {
    let mut attempt = 0u32;
    loop {
        match op() {
            Err(RepositoryError::DatabaseError(e))
                if attempt + 1 < BUSY_MAX_ATTEMPTS
                    && e.downcast_ref::<rusqlite::Error>()
                        .is_some_and(is_busy_or_locked) =>
            {
                attempt += 1;
                let backoff = BUSY_BACKOFF_BASE * attempt;
                tracing::warn!(
                    attempt,
                    backoff_ms = backoff.as_millis() as u64,
                    "database busy/locked; retrying write after backoff"
                );
                std::thread::sleep(backoff);
            }
            other => return other,
        }
    }
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
            let repo = SqliteIssuerRepository::new(*h);
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
        use chrono::SubsecRound;

        let (_db, repo) = test_repo();
        let id = IssuerId::new();
        let original = Issuer::builder().id(id).build().unwrap();
        // Stored timestamps are canonicalized to millisecond precision (truncated), so compare
        // against the millis-truncated instant (the invariant under test is immutability across a
        // full replace, not sub-millisecond fidelity).
        let created_at = original.created_at().trunc_subsecs(3);
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

    // ---- list_paged (keyset pagination) -------------------------------------------------------

    /// Insert `n` issuers and return their ids sorted ascending (the order `list_paged` yields).
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

    // ---- created_at canonicalization ----------------------------------------------------------

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
                params![issuer.id().as_bytes()],
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

        let found = repo.find_by_id(issuer.id()).unwrap().unwrap();
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
                params![id.as_bytes(), "2026-01-02T03:04:05+00:00"],
            )
            .unwrap();
        }

        let found = repo
            .find_by_id(&id)
            .unwrap()
            .expect("legacy row must parse");
        assert_eq!(
            found.data.created_at().to_rfc3339(),
            "2026-01-02T03:04:05+00:00"
        );
    }

    // ---- busy/locked retry --------------------------------------------------------------------

    use crate::sqlite::db::Database;
    use std::cell::Cell;

    /// Build a `RepositoryError::Backend` wrapping a SQLITE_BUSY failure (as the retry helper sees).
    fn busy_backend_error() -> RepositoryError {
        let ffi = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY);
        backend(rusqlite::Error::SqliteFailure(
            ffi,
            Some("database is locked".into()),
        ))
    }

    #[test]
    fn with_busy_retry_succeeds_after_transient_busy() {
        let attempts = Cell::new(0u32);
        let result = with_busy_retry(|| {
            let n = attempts.get() + 1;
            attempts.set(n);
            if n < 3 {
                Err(busy_backend_error()) // busy on the first two attempts
            } else {
                Ok(42)
            }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.get(), 3, "should have retried until success");
    }

    #[test]
    fn with_busy_retry_gives_up_after_max_attempts() {
        let attempts = Cell::new(0u32);
        let result: RepositoryResult<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(busy_backend_error()) // always busy
        });
        assert!(matches!(result, Err(RepositoryError::DatabaseError(_))));
        assert_eq!(
            attempts.get(),
            BUSY_MAX_ATTEMPTS,
            "should stop after BUSY_MAX_ATTEMPTS"
        );
    }

    #[test]
    fn with_busy_retry_does_not_retry_non_busy_errors() {
        let attempts = Cell::new(0u32);
        let result: RepositoryResult<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(RepositoryError::Conflict("nope".into())) // not a busy/locked condition
        });
        assert!(matches!(result, Err(RepositoryError::Conflict(_))));
        assert_eq!(attempts.get(), 1, "non-busy errors must not be retried");
    }

    #[test]
    fn list_paged_on_empty_table_is_empty() {
        let (_db, repo) = test_repo();
        let page = repo.list_paged(None, 10).unwrap();
        assert!(page.is_empty());
    }

    #[test]
    fn list_paged_first_page_respects_limit_and_order() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 5);

        let page = repo.list_paged(None, 2).unwrap();
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

        // Page through with limit 2: [0,1], [2,3], [4], [].
        let mut collected: Vec<IssuerId> = Vec::new();
        let mut after: Option<IssuerId> = None;
        loop {
            let page = repo.list_paged(after, 2).unwrap();
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

        let page = repo.list_paged(ids.last().copied(), 10).unwrap();
        assert!(page.is_empty(), "seeking past the last id yields no rows");
    }

    #[test]
    fn list_paged_middle_seek_returns_strictly_after() {
        let (_db, repo) = test_repo();
        let ids = insert_sorted_ids(&repo, 5);

        // Seek after ids[1] → should return ids[2], ids[3] (limit 2).
        let page = repo.list_paged(Some(ids[1]), 2).unwrap();
        let page_ids: Vec<IssuerId> = page.iter().map(|v| *v.data.id()).collect();
        assert_eq!(page_ids, ids[2..4]);
    }
}

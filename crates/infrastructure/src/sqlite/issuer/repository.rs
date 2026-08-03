use std::time::Duration;

use crate::sqlite::connection::{Db, DbHandle};
use crate::sqlite::issuer::queries;
use ftracker_identifiers::{Cnpj, Lei};
use rusqlite::{Connection, OptionalExtension, params};
use valqeron_core::{
    Issuer, IssuerId, IssuerPatch, IssuerRepository, RepositoryResult, StorageFault, Versioned,
    WriteOutcome,
};

const BUSY_MAX_ATTEMPTS: u32 = 5;

const BUSY_BACKOFF_BASE: Duration = Duration::from_millis(10);

pub struct SqliteIssuerRepository {
    db: DbHandle,
}

impl SqliteIssuerRepository {
    pub(crate) fn new(db: DbHandle) -> Self {
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

    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists_by_cnpj(&conn, cnpj).map_err(backend)
    }

    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists_by_lei(&conn, lei).map_err(backend)
    }

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            queries::insert(&conn, issuer).map(|_| ())
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
            match queries::apply_patch(&conn, id, expected_version, &patch)? {
                0 => write_outcome(&conn, id, expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        let id = issuer.id();
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::update(&conn, issuer, expected_version)? {
                0 => write_outcome(&conn, id, expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::delete(&conn, id, expected_version)? {
                0 => write_outcome(&conn, id, expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(backend)
    }
}

fn backend(e: rusqlite::Error) -> StorageFault {
    StorageFault::new(e)
}

fn is_busy_or_locked(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::DatabaseBusy
                || e.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

fn with_busy_retry<T>(op: impl Fn() -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    let mut attempt = 0u32;
    loop {
        match op() {
            Err(e) if attempt.saturating_add(1) < BUSY_MAX_ATTEMPTS && is_busy_or_locked(&e) => {
                attempt = attempt.saturating_add(1);
                let backoff = BUSY_BACKOFF_BASE.saturating_mul(attempt);
                tracing::warn!(
                    attempt,
                    backoff_ms = u64::try_from(backoff.as_millis()).unwrap_or(u64::MAX),
                    "database busy/locked; retrying write after backoff"
                );
                std::thread::sleep(backoff);
            }
            other => return other,
        }
    }
}

fn write_outcome(
    conn: &Connection,
    id: &IssuerId,
    expected_version: u32,
) -> rusqlite::Result<WriteOutcome> {
    let actual: Option<u32> = conn
        .prepare_cached("SELECT version FROM issuer WHERE id = ?1")?
        .query_row(params![id.as_bytes()], |row| row.get(0))
        .optional()?;

    Ok(match actual {
        Some(actual) => WriteOutcome::VersionMismatch {
            expected: expected_version,
            actual,
        },
        None => WriteOutcome::Missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connection::Database;
    use chrono::Utc;
    use ftracker_identifiers::{Cnpj, CountryCode, Lei};
    use std::str::FromStr;
    use valqeron_core::{IssuerName, IssuerStatus};

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

        assert_eq!(
            repo.apply_patch(issuer.id(), 1, patch).unwrap(),
            WriteOutcome::Applied
        );

        let updated = repo.find_by_id(issuer.id()).unwrap().unwrap();
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
        assert!(repo.find_by_id(issuer.id()).unwrap().is_none());
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
        assert!(repo.find_by_id(issuer.id()).unwrap().is_some());
    }

    #[test]
    fn delete_missing_id_reports_missing() {
        let (_db, repo) = test_repo();
        let outcome = repo.delete(&IssuerId::new(), 1).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
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
        assert_eq!(repo.update(&replacement, 1).unwrap(), WriteOutcome::Applied);

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
        let created_at = original.created_at().trunc_subsecs(3);
        repo.insert(&original).unwrap();

        let replacement = Issuer::builder()
            .id(id)
            .status(IssuerStatus::Retired)
            .build()
            .unwrap();
        assert_eq!(repo.update(&replacement, 1).unwrap(), WriteOutcome::Applied);

        let found = repo.find_by_id(&id).unwrap().unwrap();
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

        let issuer = Issuer::reconstitute(
            id,
            IssuerStatus::Active,
            Utc::now(),
            Some(IssuerName::new("Foreign CNPJ Corp").unwrap()),
            Some(Cnpj::new("12345678000195").unwrap()),
            None,
            Some(CountryCode::from_str("US").unwrap()),
        );

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

    use std::cell::Cell;

    fn busy_error() -> rusqlite::Error {
        let ffi = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY);
        rusqlite::Error::SqliteFailure(ffi, Some("database is locked".into()))
    }

    #[test]
    fn with_busy_retry_succeeds_after_transient_busy() {
        let attempts = Cell::new(0u32);
        let result = with_busy_retry(|| {
            let n = attempts.get() + 1;
            attempts.set(n);
            if n < 3 { Err(busy_error()) } else { Ok(42) }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.get(), 3, "should have retried until success");
    }

    #[test]
    fn with_busy_retry_gives_up_after_max_attempts() {
        let attempts = Cell::new(0u32);
        let result: rusqlite::Result<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(busy_error()) // always busy
        });
        assert!(result.is_err());
        assert_eq!(
            attempts.get(),
            BUSY_MAX_ATTEMPTS,
            "should stop after BUSY_MAX_ATTEMPTS"
        );
    }

    #[test]
    fn with_busy_retry_does_not_retry_non_busy_errors() {
        let attempts = Cell::new(0u32);
        let result: rusqlite::Result<()> = with_busy_retry(|| {
            attempts.set(attempts.get() + 1);
            Err(rusqlite::Error::QueryReturnedNoRows)
        });
        assert!(matches!(result, Err(rusqlite::Error::QueryReturnedNoRows)));
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

        let page = repo.list_paged(Some(ids[1]), 2).unwrap();
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
}

use std::collections::HashMap;

use crate::sqlite::connection::{Db, DbHandle};
use crate::sqlite::issuer::queries;
use crate::sqlite::security::model::SecurityRow;
use crate::sqlite::security::queries as security_queries;
use crate::sqlite::support::{create_storage_fault_from_error, with_busy_retry, write_outcome};
use uuid::Uuid;
use valqeron_core::{
    Issuer, IssuerId, IssuerPatch, IssuerRepository, IssuerSnapshot, LoadMode, Loading,
    RepositoryResult, Security, Versioned, WriteOutcome,
};
use valqeron_identifiers::{Cnpj, Lei};

use crate::sqlite::issuer::model::IssuerRow;

const ISSUER_VERSION_SQL: &str = "SELECT version FROM issuer WHERE id = ?1";

pub struct SqliteIssuerRepository {
    db: DbHandle,
}

impl SqliteIssuerRepository {
    pub(crate) fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

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
        let Some(row) = queries::find_by_id(&conn, id).map_err(create_storage_fault_from_error)? else {
            return Ok(None);
        };

        match mode {
            LoadMode::Lazy => Ok(Some(reconstitute_lazy(row))),
            LoadMode::Eager => {
                let securities = security_queries::list_by_issuer(&conn, id)
                    .map_err(create_storage_fault_from_error)?
                    .into_iter()
                    .map(|row| row.into_inner().data)
                    .collect();
                Ok(Some(reconstitute_with(row.into_inner(), securities)))
            }
        }
    }

    fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>> {
        let conn = self.db.read();
        let rows = queries::list_all(&conn).map_err(create_storage_fault_from_error)?;

        match mode {
            LoadMode::Lazy => Ok(rows.into_iter().map(reconstitute_lazy).collect()),
            LoadMode::Eager => {
                let securities = queries::securities_for_all_issuers(&conn).map_err(create_storage_fault_from_error)?;
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
        let rows = queries::list_paged(&conn, after.as_ref(), limit).map_err(create_storage_fault_from_error)?;

        match mode {
            LoadMode::Lazy => Ok(rows.into_iter().map(reconstitute_lazy).collect()),
            LoadMode::Eager => {
                let securities = queries::securities_for_issuer_page(&conn, after.as_ref(), limit)
                    .map_err(create_storage_fault_from_error)?;
                Ok(hydrate_rows(rows, securities))
            }
        }
    }

    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists(&conn, id).map_err(create_storage_fault_from_error)
    }

    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists_by_cnpj(&conn, cnpj).map_err(create_storage_fault_from_error)
    }

    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists_by_lei(&conn, lei).map_err(create_storage_fault_from_error)
    }

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            queries::insert(&conn, issuer).map(|_| ())
        })
        .map_err(create_storage_fault_from_error)
    }

    fn apply_patch(
        &self,
        issuer_id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::apply_patch(&conn, issuer_id, expected_version, &patch)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, issuer_id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(create_storage_fault_from_error)
    }

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        let id = issuer.id();
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::update(&conn, issuer, expected_version)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(create_storage_fault_from_error)
    }

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::delete(&conn, id, expected_version)? {
                0 => write_outcome(&conn, ISSUER_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(create_storage_fault_from_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connection::Database;
    use crate::sqlite::security::SqliteSecurityRepository;
    use chrono::Utc;
    use std::str::FromStr;
    use valqeron_core::{
        IssuerName, IssuerStatus, SecurityId, SecurityKind, SecurityRepository, SecurityStatus,
    };
    use valqeron_identifiers::{Cnpj, CountryCode, Lei};

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
        let db = Database::open_in_memory().unwrap();
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
            .name(valqeron_core::SecurityName::new("Acme PN").unwrap())
            .isin(valqeron_identifiers::Isin::new("BRVALEACNOR0").unwrap())
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

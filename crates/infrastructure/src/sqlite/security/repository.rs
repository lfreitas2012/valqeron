use crate::sqlite::connection::{Db, DbHandle};
use crate::sqlite::security::queries;
use crate::sqlite::support::{create_storage_fault_from_error, with_busy_retry, write_outcome};
use valqeron_core::{
    IssuerId, RepositoryResult, Security, SecurityId, SecurityPatch, SecurityRepository, Versioned,
    WriteOutcome,
};
use valqeron_identifiers::Isin;

const SECURITY_VERSION_SQL: &str = "SELECT version FROM security WHERE id = ?1";

pub struct SqliteSecurityRepository {
    db: DbHandle,
}

impl SqliteSecurityRepository {
    pub(crate) fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

impl SecurityRepository for SqliteSecurityRepository {
    fn find_by_id(&self, id: &SecurityId) -> RepositoryResult<Option<Versioned<Security>>> {
        let conn = self.db.read();
        queries::find_by_id(&conn, id)
            .map(|opt| opt.map(|row| row.into_inner()))
            .map_err(create_storage_fault_from_error)
    }

    fn find_by_isin(&self, isin: &Isin) -> RepositoryResult<Option<Versioned<Security>>> {
        let conn = self.db.read();
        queries::find_by_isin(&conn, isin)
            .map(|opt| opt.map(|row| row.into_inner()))
            .map_err(create_storage_fault_from_error)
    }

    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Security>>> {
        let conn = self.db.read();

        queries::list_all(&conn)
            .map(|rows| rows.into_iter().map(|row| row.into_inner()).collect())
            .map_err(create_storage_fault_from_error)
    }

    fn list_by_issuer(&self, issuer_id: &IssuerId) -> RepositoryResult<Vec<Versioned<Security>>> {
        let conn = self.db.read();

        queries::list_by_issuer(&conn, issuer_id)
            .map(|rows| rows.into_iter().map(|row| row.into_inner()).collect())
            .map_err(create_storage_fault_from_error)
    }

    fn list_paged(
        &self,
        after: Option<SecurityId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Security>>> {
        let conn = self.db.read();

        queries::list_paged(&conn, after.as_ref(), limit)
            .map(|rows| rows.into_iter().map(|row| row.into_inner()).collect())
            .map_err(create_storage_fault_from_error)
    }

    fn exists(&self, id: &SecurityId) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists(&conn, id).map_err(create_storage_fault_from_error)
    }

    fn exists_by_isin(&self, isin: &Isin) -> RepositoryResult<bool> {
        let conn = self.db.read();
        queries::exists_by_isin(&conn, isin).map_err(create_storage_fault_from_error)
    }

    fn insert(&self, security: &Security) -> RepositoryResult<()> {
        with_busy_retry(|| {
            let conn = self.db.write();
            queries::insert(&conn, security).map(|_| ())
        })
        .map_err(create_storage_fault_from_error)
    }

    fn apply_patch(
        &self,
        security_id: &SecurityId,
        expected_version: u32,
        patch: SecurityPatch,
    ) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::apply_patch(&conn, security_id, expected_version, &patch)? {
                0 => write_outcome(&conn, SECURITY_VERSION_SQL, security_id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(create_storage_fault_from_error)
    }

    fn update(&self, security: &Security, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        let id = security.id();
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::update(&conn, security, expected_version)? {
                0 => write_outcome(&conn, SECURITY_VERSION_SQL, id.as_bytes(), expected_version),
                _ => Ok(WriteOutcome::Applied),
            }
        })
        .map_err(create_storage_fault_from_error)
    }

    fn delete(&self, id: &SecurityId, expected_version: u32) -> RepositoryResult<WriteOutcome> {
        with_busy_retry(|| {
            let conn = self.db.write();
            match queries::delete(&conn, id, expected_version)? {
                0 => write_outcome(&conn, SECURITY_VERSION_SQL, id.as_bytes(), expected_version),
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
    use crate::sqlite::issuer::SqliteIssuerRepository;
    use valqeron_core::{
        DepositaryReceiptRatio, Issuer, IssuerRepository, SecurityKind, SecurityName, SecuritySnapshot,
        SecurityStatus,
    };
    use valqeron_identifiers::Cfi;

    fn test_repo() -> (Database, SqliteSecurityRepository, IssuerId) {
        let db = Database::open_in_memory().unwrap();
        let issuers = SqliteIssuerRepository::new(db.handle());
        let issuer = Issuer::builder().build().unwrap();
        issuers.insert(&issuer).unwrap();
        let repo = SqliteSecurityRepository::new(db.handle());
        (db, repo, *issuer.id())
    }

    #[test]
    fn insert_then_find_round_trips_and_defaults_to_version_1() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .name(SecurityName::new("Vale ON").unwrap())
            .isin(Isin::new("BRVALEACNOR0").unwrap())
            .cfi(Cfi::new("ESVUFR").unwrap())
            .build()
            .unwrap();

        repo.insert(&security).unwrap();

        let found = repo
            .find_by_id(security.id())
            .unwrap()
            .expect("Security should be found");
        assert_eq!(
            found.version, 1,
            "New insertions should default to version 1"
        );
        assert_eq!(found.data.id(), security.id());
        assert_eq!(found.data.issuer_id(), &issuer_id);
        assert_eq!(found.data.kind(), SecurityKind::CommonShare);
        assert_eq!(found.data.isin(), security.isin());
        assert_eq!(found.data.cfi(), security.cfi());
    }

    #[test]
    fn insert_depositary_receipt_round_trips_underlying_and_ratio() {
        let (_db, repo, issuer_id) = test_repo();
        let underlying = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&underlying).unwrap();

        let adr = Security::builder(issuer_id, SecurityKind::DepositaryReceipt)
            .underlying_security_id(*underlying.id())
            .dr_ratio(DepositaryReceiptRatio::new(1, 2).unwrap())
            .build()
            .unwrap();
        repo.insert(&adr).unwrap();

        let found = repo.find_by_id(adr.id()).unwrap().unwrap();
        assert_eq!(found.data.underlying_security_id(), Some(underlying.id()));
        assert_eq!(found.data.dr_ratio(), Some(&DepositaryReceiptRatio::new(1, 2).unwrap()));
    }

    #[test]
    fn insert_with_unknown_issuer_is_a_storage_fault() {
        let db = Database::open_in_memory().unwrap();
        let repo = SqliteSecurityRepository::new(db.handle());

        let security = Security::builder(IssuerId::new(), SecurityKind::CommonShare)
            .build()
            .unwrap();

        assert!(
            repo.insert(&security).is_err(),
            "a foreign-key violation must surface as a storage fault"
        );
    }

    #[test]
    fn insert_duplicate_isin_is_a_storage_fault() {
        let (_db, repo, issuer_id) = test_repo();
        let isin = Isin::new("BRVALEACNOR0").unwrap();

        let a = Security::builder(issuer_id, SecurityKind::CommonShare)
            .isin(isin.clone())
            .build()
            .unwrap();
        let b = Security::builder(issuer_id, SecurityKind::PreferredShare)
            .isin(isin)
            .build()
            .unwrap();

        repo.insert(&a).unwrap();
        assert!(
            repo.insert(&b).is_err(),
            "a UNIQUE collision on isin must surface as a storage fault"
        );
    }

    #[test]
    fn insert_reconstituted_dr_fields_on_common_share_is_a_storage_fault() {
        let (_db, repo, issuer_id) = test_repo();

        // Bypasses builder validation; the schema CHECK is the safety net.
        let security = Security::reconstitute(SecuritySnapshot {
            id: SecurityId::new(),
            issuer_id,
            kind: SecurityKind::CommonShare,
            status: SecurityStatus::Active,
            created_at: chrono::Utc::now(),
            name: None,
            isin: None,
            cfi: None,
            underlying_security_id: None,
            dr_ratio: Some(DepositaryReceiptRatio::new(1, 1).unwrap()),
        });

        assert!(
            repo.insert(&security).is_err(),
            "a CHECK violation must surface as a storage fault"
        );
    }

    #[test]
    fn find_by_isin_and_exists_by_isin_report_stored_identifiers() {
        let (_db, repo, issuer_id) = test_repo();
        let isin = Isin::new("BRVALEACNOR0").unwrap();
        let other = Isin::new("US91912E1055").unwrap();

        assert!(repo.find_by_isin(&isin).unwrap().is_none());
        assert!(!repo.exists_by_isin(&isin).unwrap());

        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .isin(isin.clone())
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        let found = repo.find_by_isin(&isin).unwrap().unwrap();
        assert_eq!(found.data.id(), security.id());
        assert!(repo.exists_by_isin(&isin).unwrap());
        assert!(!repo.exists_by_isin(&other).unwrap());
    }

    #[test]
    fn list_by_issuer_returns_only_that_issuers_securities() {
        let (db, repo, issuer_a) = test_repo();

        let issuers = SqliteIssuerRepository::new(db.handle());
        let other = Issuer::builder().build().unwrap();
        issuers.insert(&other).unwrap();
        let issuer_b = *other.id();

        let a1 = Security::builder(issuer_a, SecurityKind::CommonShare)
            .build()
            .unwrap();
        let a2 = Security::builder(issuer_a, SecurityKind::PreferredShare)
            .build()
            .unwrap();
        let b1 = Security::builder(issuer_b, SecurityKind::Unit)
            .build()
            .unwrap();
        repo.insert(&a1).unwrap();
        repo.insert(&a2).unwrap();
        repo.insert(&b1).unwrap();

        let listed = repo.list_by_issuer(&issuer_a).unwrap();
        let mut ids: Vec<SecurityId> = listed.iter().map(|v| *v.data.id()).collect();
        ids.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));

        let mut expected = vec![*a1.id(), *a2.id()];
        expected.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));

        assert_eq!(ids, expected);
    }

    #[test]
    fn apply_patch_bumps_version_and_updates_ratio_pair() {
        let (_db, repo, issuer_id) = test_repo();
        let underlying = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&underlying).unwrap();

        let adr = Security::builder(issuer_id, SecurityKind::DepositaryReceipt)
            .underlying_security_id(*underlying.id())
            .dr_ratio(DepositaryReceiptRatio::new(1, 1).unwrap())
            .build()
            .unwrap();
        repo.insert(&adr).unwrap();

        let patch = SecurityPatch::builder()
            .status(SecurityStatus::Retired)
            .dr_ratio(DepositaryReceiptRatio::new(2, 1).unwrap())
            .build();

        assert_eq!(
            repo.apply_patch(adr.id(), 1, patch).unwrap(),
            WriteOutcome::Applied
        );

        let updated = repo.find_by_id(adr.id()).unwrap().unwrap();
        assert!(updated.data.status().is_retired());
        assert_eq!(updated.data.dr_ratio(), Some(&DepositaryReceiptRatio::new(2, 1).unwrap()));
        assert_eq!(updated.version, 2, "Version should be incremented to 2");
    }

    #[test]
    fn apply_patch_reports_version_mismatch_on_stale_version() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        let patch = SecurityPatch::builder()
            .status(SecurityStatus::Retired)
            .build();

        let outcome = repo.apply_patch(security.id(), 99, patch).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            }
        );
    }

    #[test]
    fn apply_patch_on_missing_id_reports_missing() {
        let (_db, repo, _issuer_id) = test_repo();
        let patch = SecurityPatch::builder()
            .status(SecurityStatus::Retired)
            .build();

        let outcome = repo.apply_patch(&SecurityId::new(), 1, patch).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn update_replaces_mutable_fields_and_preserves_identity_facts() {
        let (_db, repo, issuer_id) = test_repo();
        let id = SecurityId::new();

        let original = Security::builder(issuer_id, SecurityKind::CommonShare)
            .id(id)
            .name(SecurityName::new("Vale ON").unwrap())
            .isin(Isin::new("BRVALEACNOR0").unwrap())
            .build()
            .unwrap();
        repo.insert(&original).unwrap();

        let replacement = Security::builder(issuer_id, SecurityKind::CommonShare)
            .id(id)
            .status(SecurityStatus::Retired)
            .build()
            .unwrap();
        assert_eq!(repo.update(&replacement, 1).unwrap(), WriteOutcome::Applied);

        let found = repo.find_by_id(&id).unwrap().unwrap();
        assert_eq!(found.version, 2, "version should bump");
        assert!(found.data.status().is_retired());
        assert!(
            found.data.name().is_none(),
            "unset name must be cleared to NULL (full replace, not patch)"
        );
        assert!(
            found.data.isin().is_none(),
            "unset isin must be cleared to NULL (full replace, not patch)"
        );
        assert_eq!(
            found.data.issuer_id(),
            &issuer_id,
            "issuer_id is an identity fact and must survive an update"
        );
        assert_eq!(found.data.kind(), SecurityKind::CommonShare);
    }

    #[test]
    fn update_with_stale_version_reports_version_mismatch() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        let outcome = repo.update(&security, 99).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            }
        );
    }

    #[test]
    fn update_on_missing_id_reports_missing() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        let outcome = repo.update(&security, 1).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn delete_with_correct_version_removes_the_row() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        assert_eq!(
            repo.delete(security.id(), 1).unwrap(),
            WriteOutcome::Applied
        );
        assert!(repo.find_by_id(security.id()).unwrap().is_none());
    }

    #[test]
    fn delete_with_stale_version_reports_version_mismatch() {
        let (_db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        let outcome = repo.delete(security.id(), 99).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 99,
                actual: 1
            }
        );
        assert!(repo.find_by_id(security.id()).unwrap().is_some());
    }

    #[test]
    fn delete_missing_id_reports_missing() {
        let (_db, repo, _issuer_id) = test_repo();
        let outcome = repo.delete(&SecurityId::new(), 1).unwrap();
        assert_eq!(outcome, WriteOutcome::Missing);
    }

    #[test]
    fn dry_run_repository_writes_are_rolled_back() {
        let (db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();

        db.dry_run(|h| {
            let dry_repo = SqliteSecurityRepository::new(h.clone());
            dry_repo.insert(&security).unwrap();
            assert!(dry_repo.find_by_id(security.id()).unwrap().is_some());
        })
        .unwrap();

        assert!(
            repo.find_by_id(security.id()).unwrap().is_none(),
            "dry-run insert must not persist"
        );
    }

    fn insert_sorted_ids(
        repo: &SqliteSecurityRepository,
        issuer_id: IssuerId,
        n: usize,
    ) -> Vec<SecurityId> {
        let mut ids: Vec<SecurityId> = Vec::with_capacity(n);
        for _ in 0..n {
            let security = Security::builder(issuer_id, SecurityKind::CommonShare)
                .build()
                .unwrap();
            repo.insert(&security).unwrap();
            ids.push(*security.id());
        }
        ids.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        ids
    }

    #[test]
    fn list_paged_walks_all_pages_via_keyset_without_gaps_or_dupes() {
        let (_db, repo, issuer_id) = test_repo();
        let ids = insert_sorted_ids(&repo, issuer_id, 5);

        let mut collected: Vec<SecurityId> = Vec::new();
        let mut after: Option<SecurityId> = None;
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
    fn list_all_returns_everything_in_id_order() {
        let (_db, repo, issuer_id) = test_repo();
        let ids = insert_sorted_ids(&repo, issuer_id, 3);

        let listed: Vec<SecurityId> = repo
            .list_all()
            .unwrap()
            .iter()
            .map(|v| *v.data.id())
            .collect();
        assert_eq!(listed, ids);
    }

    #[test]
    fn deleting_issuer_with_securities_is_a_storage_fault() {
        let (db, repo, issuer_id) = test_repo();
        let security = Security::builder(issuer_id, SecurityKind::CommonShare)
            .build()
            .unwrap();
        repo.insert(&security).unwrap();

        let issuers = SqliteIssuerRepository::new(db.handle());
        assert!(
            issuers.delete(&issuer_id, 1).is_err(),
            "the foreign key must forbid deleting an issuer that still has securities"
        );
    }
}

use crate::common::{RepositoryResult, Versioned, WriteOutcome};
use crate::issuer::IssuerId;
use crate::security::Security;
use crate::security::patch::SecurityPatch;
use std::rc::Rc;
use std::sync::Arc;
use valqeron_identifiers::Isin;

#[cfg_attr(test, mockall::automock)]
pub trait SecurityRepository {
    fn find_by_isin(&self, isin: &Isin) -> RepositoryResult<Option<Versioned<Security>>>;

    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Security>>>;

    fn list_by_issuer(&self, issuer_id: &IssuerId) -> RepositoryResult<Vec<Versioned<Security>>>;

    fn list_paged(
        &self,
        after: Option<Isin>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Security>>>;

    fn exists_by_isin(&self, isin: &Isin) -> RepositoryResult<bool>;

    fn insert(&self, security: &Security) -> RepositoryResult<()>;

    fn apply_patch(
        &self,
        isin: &Isin,
        expected_version: u32,
        patch: SecurityPatch,
    ) -> RepositoryResult<WriteOutcome>;

    fn update(&self, security: &Security, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    fn delete(&self, isin: &Isin, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

macro_rules! delegate_security_repository {
    ($ty:ty) => {
        impl<R: SecurityRepository + ?Sized> SecurityRepository for $ty {
            fn find_by_isin(&self, isin: &Isin) -> RepositoryResult<Option<Versioned<Security>>> {
                (**self).find_by_isin(isin)
            }
            fn list_all(&self) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_all()
            }
            fn list_by_issuer(
                &self,
                issuer_id: &IssuerId,
            ) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_by_issuer(issuer_id)
            }
            fn list_paged(
                &self,
                after: Option<Isin>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_paged(after, limit)
            }
            fn exists_by_isin(&self, isin: &Isin) -> RepositoryResult<bool> {
                (**self).exists_by_isin(isin)
            }
            fn insert(&self, security: &Security) -> RepositoryResult<()> {
                (**self).insert(security)
            }
            fn apply_patch(
                &self,
                isin: &Isin,
                expected_version: u32,
                patch: SecurityPatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(isin, expected_version, patch)
            }
            fn update(
                &self,
                security: &Security,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(security, expected_version)
            }
            fn delete(&self, isin: &Isin, expected_version: u32) -> RepositoryResult<WriteOutcome> {
                (**self).delete(isin, expected_version)
            }
        }
    };
}

delegate_security_repository!(Box<R>);
delegate_security_repository!(Rc<R>);
delegate_security_repository!(Arc<R>);

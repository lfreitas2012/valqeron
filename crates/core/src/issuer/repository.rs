use crate::issuer::patch::IssuerPatch;
use crate::issuer::{Issuer, IssuerId};
use std::rc::Rc;
use std::sync::Arc;
use valqeron_identifiers::{Cnpj, Lei};

use crate::common::{LoadMode, RepositoryResult, Versioned, WriteOutcome};

#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    fn find_by_id(
        &self,
        id: &IssuerId,
        mode: LoadMode,
    ) -> RepositoryResult<Option<Versioned<Issuer>>>;

    fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    fn list_paged(
        &self,
        after: Option<IssuerId>,
        limit: u32,
        mode: LoadMode,
    ) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool>;

    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool>;

    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool>;

    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()>;

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<WriteOutcome>;

    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

macro_rules! delegate_issuer_repository {
    ($ty:ty) => {
        impl<R: IssuerRepository + ?Sized> IssuerRepository for $ty {
            fn find_by_id(
                &self,
                id: &IssuerId,
                mode: LoadMode,
            ) -> RepositoryResult<Option<Versioned<Issuer>>> {
                (**self).find_by_id(id, mode)
            }
            fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_all(mode)
            }
            fn list_paged(
                &self,
                after: Option<IssuerId>,
                limit: u32,
                mode: LoadMode,
            ) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_paged(after, limit, mode)
            }
            fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool> {
                (**self).exists_by_cnpj(cnpj)
            }
            fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool> {
                (**self).exists_by_lei(lei)
            }
            fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
                (**self).insert(issuer)
            }
            fn apply_patch(
                &self,
                id: &IssuerId,
                expected_version: u32,
                patch: IssuerPatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(
                &self,
                issuer: &Issuer,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(issuer, expected_version)
            }
            fn delete(
                &self,
                id: &IssuerId,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_issuer_repository!(Box<R>);
delegate_issuer_repository!(Rc<R>);
delegate_issuer_repository!(Arc<R>);

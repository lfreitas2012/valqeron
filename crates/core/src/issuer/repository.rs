use crate::issuer::error::RepositoryError;
use crate::issuer::patch::IssuerPatch;
use crate::issuer::{Issuer, IssuerId};
use std::rc::Rc;
use std::sync::Arc;

use crate::common::Versioned;

/// Result alias for repository operations.
pub type RepositoryResult<T> = Result<T, RepositoryError>;

/// Persistence operations for [`Issuer`]. Mutations use optimistic locking via an expected version.
#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    /// Fetch an issuer with its current version, or `None` if absent.
    fn find_by_id(&self, id: &IssuerId) -> RepositoryResult<Option<Versioned<Issuer>>>;

    /// List all registered issuers.
    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    /// Whether an issuer with `id` exists.
    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool>;

    /// Insert a new issuer. Returns `Conflict` on a duplicate id or CNPJ/LEI.
    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()>;

    /// Apply a partial update, bumping the version. Returns `Conflict` if `expected_version` is
    /// stale, `NotFound` if the issuer is absent.
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<()>;

    /// Fully replace an issuer's mutable fields (clearing unset optionals to NULL), bumping the version.
    /// Unlike [`apply_patch`](Self::apply_patch), this overwrites every mutable column.
    ///
    /// `id` and `created_at` are immutable and left untouched.
    ///
    /// Returns `Conflict` if `expected_version` is stale, `NotFound` if the issuer is absent.
    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<()>;

    /// Delete an issuer. Returns `Conflict` if `expected_version` is stale, `NotFound` if the issuer is absent.
    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<()>;
}

macro_rules! delegate_issuer_repository {
    ($ty:ty) => {
        impl<R: IssuerRepository + ?Sized> IssuerRepository for $ty {
            fn find_by_id(&self, id: &IssuerId) -> RepositoryResult<Option<Versioned<Issuer>>> {
                (**self).find_by_id(id)
            }
            fn list_all(&self) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_all()
            }
            fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
                (**self).insert(issuer)
            }
            fn apply_patch(
                &self,
                id: &IssuerId,
                expected_version: u32,
                patch: IssuerPatch,
            ) -> RepositoryResult<()> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<()> {
                (**self).update(issuer, expected_version)
            }
            fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<()> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_issuer_repository!(Box<R>);
delegate_issuer_repository!(Rc<R>);
delegate_issuer_repository!(Arc<R>);

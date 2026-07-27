use crate::issuer::error::IssuerRepositoryError;
use crate::issuer::patch::IssuerPatch;
use crate::issuer::{Issuer, IssuerId};
use std::rc::Rc;
use std::sync::Arc;

pub mod sqlite;

use crate::common::Versioned;

/// Persistence operations for [`Issuer`]. Mutations use optimistic locking via an expected version.
#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    /// Fetch an issuer with its current version, or `None` if absent.
    fn find_by_id(&self, id: &IssuerId)
    -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError>;

    /// Whether an issuer with `id` exists.
    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError>;

    /// Insert a new issuer. Returns `Conflict` on a duplicate id or CNPJ/LEI.
    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError>;

    /// Apply a partial update, bumping the version. Returns `Conflict` if `expected_version` is
    /// stale, `NotFound` if the issuer is absent.
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError>;

    /// Fully replace an issuer's mutable fields (clearing unset optionals to NULL), bumping the version.
    /// Unlike [`apply_patch`](Self::apply_patch), this overwrites every mutable column.
    ///
    /// `id` and `created_at` are immutable and left untouched.
    ///
    /// Returns `Conflict` if `expected_version` is stale, `NotFound` if the issuer is absent.
    fn update(&self, issuer: &Issuer, expected_version: u32) -> Result<(), IssuerRepositoryError>;

    /// Delete an issuer. Returns `Conflict` if `expected_version` is stale, `NotFound` if the issuer is absent.
    fn delete(&self, id: &IssuerId, expected_version: u32) -> Result<(), IssuerRepositoryError>;
}

impl<R: IssuerRepository + ?Sized> IssuerRepository for Box<R> {
    fn find_by_id(
        &self,
        id: &IssuerId,
    ) -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError> {
        (**self).find_by_id(id)
    }
    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError> {
        (**self).exists(id)
    }
    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError> {
        (**self).insert(issuer)
    }
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError> {
        (**self).apply_patch(id, expected_version, patch)
    }
    fn update(&self, issuer: &Issuer, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).update(issuer, expected_version)
    }
    fn delete(&self, id: &IssuerId, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id, expected_version)
    }
}

impl<R: IssuerRepository + ?Sized> IssuerRepository for Rc<R> {
    fn find_by_id(
        &self,
        id: &IssuerId,
    ) -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError> {
        (**self).find_by_id(id)
    }
    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError> {
        (**self).exists(id)
    }
    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError> {
        (**self).insert(issuer)
    }
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError> {
        (**self).apply_patch(id, expected_version, patch)
    }
    fn update(&self, issuer: &Issuer, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).update(issuer, expected_version)
    }
    fn delete(&self, id: &IssuerId, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id, expected_version)
    }
}

impl<R: IssuerRepository + ?Sized> IssuerRepository for Arc<R> {
    fn find_by_id(
        &self,
        id: &IssuerId,
    ) -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError> {
        (**self).find_by_id(id)
    }
    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError> {
        (**self).exists(id)
    }
    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError> {
        (**self).insert(issuer)
    }
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError> {
        (**self).apply_patch(id, expected_version, patch)
    }
    fn update(&self, issuer: &Issuer, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).update(issuer, expected_version)
    }
    fn delete(&self, id: &IssuerId, expected_version: u32) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id, expected_version)
    }
}

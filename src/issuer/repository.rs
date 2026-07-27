use crate::issuer::error::IssuerRepositoryError;
use crate::issuer::patch::IssuerPatch;
use crate::issuer::{Issuer, IssuerId};
use std::rc::Rc;
use std::sync::Arc;

pub mod sqlite;

use crate::common::Versioned;

#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    fn find_by_id(&self, id: &IssuerId)
    -> Result<Option<Versioned<Issuer>>, IssuerRepositoryError>;

    fn exists(&self, id: &IssuerId) -> Result<bool, IssuerRepositoryError>;

    fn insert(&self, issuer: &Issuer) -> Result<(), IssuerRepositoryError>;

    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> Result<(), IssuerRepositoryError>;

    fn delete(&self, id: &IssuerId) -> Result<(), IssuerRepositoryError>;
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
    fn delete(&self, id: &IssuerId) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id)
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
    fn delete(&self, id: &IssuerId) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id)
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
    fn delete(&self, id: &IssuerId) -> Result<(), IssuerRepositoryError> {
        (**self).delete(id)
    }
}

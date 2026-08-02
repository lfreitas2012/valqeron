use crate::issuer::patch::IssuerPatch;
use crate::issuer::{Issuer, IssuerId};
use crate::storage::StorageFault;
use ftracker_identifiers::{Cnpj, Lei};
use std::rc::Rc;
use std::sync::Arc;

use crate::common::Versioned;

/// Result alias for repository operations.
///
/// The only failure a repository surfaces is an opaque [`StorageFault`] (an infrastructure-level
/// problem). Domain outcomes such as "not found" or a stale-version write are **values**
/// ([`Option`], [`WriteOutcome`]), not errors, so callers branch on them without ever inspecting a
/// driver-specific error.
pub type RepositoryResult<T> = Result<T, StorageFault>;

/// The outcome of a version-guarded write (`apply_patch`, `update`, `delete`).
///
/// Optimistic concurrency is a domain concept, not a storage error: a write that matches no row is
/// an ordinary, expected result the caller inspects. This maps cleanly across backends (SQLite
/// affected-rows, MongoDB `matchedCount`, etc.) without any "conflict" vocabulary leaking from a
/// particular driver.
#[must_use = "a write outcome may indicate the write did not apply"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The write applied to exactly the expected row and bumped its version.
    Applied,

    /// The row exists but its version did not match the expected one; nothing was written.
    VersionMismatch { expected: u32, actual: u32 },

    /// No row with the given id exists; nothing was written.
    Missing,
}

impl WriteOutcome {
    /// Whether the write applied.
    pub fn applied(self) -> bool {
        matches!(self, WriteOutcome::Applied)
    }
}

/// Persistence operations for [`Issuer`].
///
/// This is a pure persistence port: it stores and retrieves aggregates and reports version-guarded
/// write outcomes, but enforces no business invariants itself (uniqueness, cross-field rules) — the
/// domain owns those. Mutations use optimistic locking via an expected version, reported through
/// [`WriteOutcome`] rather than an error.
#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    /// Fetch an issuer with its current version, or `None` if absent.
    fn find_by_id(&self, id: &IssuerId) -> RepositoryResult<Option<Versioned<Issuer>>>;

    /// List all registered issuers.
    ///
    /// For large or unbounded datasets prefer [`list_paged`](Self::list_paged).
    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    /// List a single page of issuers ordered by id, using keyset (seek) pagination.
    ///
    /// Returns up to `limit` issuers whose id sorts strictly after `after` (or from the beginning
    /// when `after` is `None`), in ascending id order. To fetch the next page, pass the id of the
    /// last item from the previous page as `after`. A returned page shorter than `limit` (or empty)
    /// signals the end of the data.
    fn list_paged(
        &self,
        after: Option<IssuerId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    /// Whether an issuer with `id` exists.
    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool>;

    /// Whether any issuer already holds `cnpj`. Supports domain-level uniqueness enforcement.
    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool>;

    /// Whether any issuer already holds `lei`. Supports domain-level uniqueness enforcement.
    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool>;

    /// Insert a new issuer.
    ///
    /// The caller is responsible for having enforced domain invariants (e.g. identifier
    /// uniqueness) beforehand; the store keeps only a backstop and will surface a violation as a
    /// [`StorageFault`], not a domain outcome.
    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()>;

    /// Apply a partial update, bumping the version. See [`WriteOutcome`] for the possible results.
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<WriteOutcome>;

    /// Fully replace an issuer's mutable fields (clearing unset optionals to NULL), bumping the
    /// version. Unlike [`apply_patch`](Self::apply_patch), this overwrites every mutable column.
    ///
    /// `id` and `created_at` are immutable and left untouched. See [`WriteOutcome`] for the results.
    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    /// Delete an issuer, guarded by `expected_version`. See [`WriteOutcome`] for the results.
    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
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
            fn list_paged(
                &self,
                after: Option<IssuerId>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_paged(after, limit)
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

//! Domain use cases for issuers.
//!
//! These functions own the business invariants that the persistence port deliberately does not,
//! keeping the store a dumb persistence adapter. Front-ends (CLI, future daemon) call these instead
//! of reaching for the repository directly when an invariant must hold.

use crate::issuer::Issuer;
use crate::issuer::error::RegisterIssuerError;
use crate::issuer::repository::IssuerRepository;

/// Register a new issuer, enforcing identifier uniqueness as a domain invariant.
///
/// Uniqueness of CNPJ/LEI is checked here (a domain rule) rather than being inferred from a
/// backend-specific constraint violation. The store keeps a `UNIQUE` backstop for defense in depth
/// against races; if that backstop ever fires it surfaces as an opaque
/// [`StorageFault`](crate::StorageFault) via [`RegisterIssuerError::Storage`], not as a duplicate
/// outcome.
pub fn register_issuer<R: IssuerRepository + ?Sized>(
    repo: &R,
    issuer: &Issuer,
) -> Result<(), RegisterIssuerError> {
    if let Some(cnpj) = issuer.cnpj()
        && repo.exists_by_cnpj(cnpj)?
    {
        return Err(RegisterIssuerError::DuplicateCnpj);
    }

    if let Some(lei) = issuer.lei()
        && repo.exists_by_lei(lei)?
    {
        return Err(RegisterIssuerError::DuplicateLei);
    }

    repo.insert(issuer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issuer::repository::MockIssuerRepository;
    use ftracker_identifiers::Cnpj;

    fn issuer_with_cnpj() -> Issuer {
        Issuer::builder()
            .cnpj(Cnpj::new("12.345.678/0001-95").unwrap())
            .build()
            .unwrap()
    }

    #[test]
    fn register_inserts_when_identifiers_are_free() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(false));
        repo.expect_exists_by_lei().returning(|_| Ok(false));
        repo.expect_insert().returning(|_| Ok(()));

        let issuer = issuer_with_cnpj();
        assert!(register_issuer(&repo, &issuer).is_ok());
    }

    #[test]
    fn register_rejects_duplicate_cnpj_before_insert() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(true));
        // insert must never be called on a duplicate.
        repo.expect_insert().never();

        let issuer = issuer_with_cnpj();
        assert!(matches!(
            register_issuer(&repo, &issuer),
            Err(RegisterIssuerError::DuplicateCnpj)
        ));
    }
}

use crate::issuer::Issuer;
use crate::issuer::error::RegisterIssuerError;
use crate::issuer::repository::IssuerRepository;

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

    fn issuer_with_cnpj() -> Option<Issuer> {
        let cnpj_result = Cnpj::new("12.345.678/0001-95");
        let cnpj = cnpj_result.ok()?;
        Issuer::builder().cnpj(cnpj).build().ok()
    }

    #[test]
    fn register_inserts_when_identifiers_are_free() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(false));
        repo.expect_exists_by_lei().returning(|_| Ok(false));
        repo.expect_insert().returning(|_| Ok(()));

        let Some(issuer) = issuer_with_cnpj() else {
            return;
        };
        assert!(register_issuer(&repo, &issuer).is_ok());
    }

    #[test]
    fn register_rejects_duplicate_cnpj_before_insert() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(true));
        // insert must never be called on a duplicate.
        repo.expect_insert().never();

        let Some(issuer) = issuer_with_cnpj() else {
            return;
        };
        assert!(matches!(
            register_issuer(&repo, &issuer),
            Err(RegisterIssuerError::DuplicateCnpj)
        ));
    }
}

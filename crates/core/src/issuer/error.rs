use crate::storage::StorageFault;
use ftracker_identifiers::CountryCodeError;

#[derive(thiserror::Error, Debug)]
pub enum IssuerNameError {
    #[error("issuer name cannot be empty")]
    Empty,

    #[error("issuer name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

#[derive(thiserror::Error, Debug)]
pub enum IssuerStatusError {
    #[error("Invalid status. Must be one of: {statuses:?}", statuses = vec!["ACTIVE", "RETIRED"])]
    InvalidStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum IssuerBuilderError {
    #[error("If a CNPJ is provided, the country code must be BR (Brazil). Found: {0}")]
    InvalidCountryForCnpj(String),

    #[error("Issuer name validation failed: {0}")]
    NameError(#[from] IssuerNameError),

    #[error("country code validation failed: {0}")]
    CountryCodeError(#[from] CountryCodeError),
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterIssuerError {
    #[error("an issuer with this CNPJ already exists")]
    DuplicateCnpj,

    #[error("an issuer with this LEI already exists")]
    DuplicateLei,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

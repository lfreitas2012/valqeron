use crate::storage::StorageFault;

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
}

/// Failure to register a new issuer.
///
/// Uniqueness of identifiers is a domain invariant, enforced by the registration use case
/// ([`crate::register_issuer`]) rather than by the persistence backend. A duplicate is therefore a
/// domain outcome, distinct from an opaque [`StorageFault`].
#[derive(Debug, thiserror::Error)]
pub enum RegisterIssuerError {
    /// Another issuer already holds this CNPJ.
    #[error("an issuer with this CNPJ already exists")]
    DuplicateCnpj,

    /// Another issuer already holds this LEI.
    #[error("an issuer with this LEI already exists")]
    DuplicateLei,

    /// The persistence layer failed while checking or writing.
    #[error(transparent)]
    Storage(#[from] StorageFault),
}

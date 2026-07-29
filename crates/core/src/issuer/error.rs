use crate::issuer::IssuerId;

/// Failure modes of the [`IssuerRepository`](crate::IssuerRepository) contract.
///
/// These are domain-level outcomes — optimistic-locking conflicts and absence —
/// modeled as part of the repository contract itself. A backend maps its own
/// (driver-specific) errors onto these variants, hiding storage details behind
/// [`RepositoryError::Backend`] so callers never depend on a concrete engine.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// The requested issuer does not exist.
    #[error("issuer {0:?} not found")]
    NotFound(IssuerId),

    /// An optimistic-lock (stale version) or uniqueness constraint was violated.
    #[error("constraint violation: {0}")]
    Conflict(String),

    /// A driver/infrastructure failure that is not a domain-level outcome.
    #[error(transparent)]
    Backend(#[from] anyhow::Error),
}

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

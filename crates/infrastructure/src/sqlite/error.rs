use valqeron_core::IssuerId;

#[derive(Debug, thiserror::Error)]
pub enum IssuerRepositoryError {
    #[error("constraint violation: {0}")]
    Conflict(String),

    #[error(transparent)]
    Infrastructure(#[from] anyhow::Error),

    #[error("issuer {0:?} not found")]
    NotFound(IssuerId),
}

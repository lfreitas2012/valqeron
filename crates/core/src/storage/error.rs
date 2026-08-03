use std::error::Error as StdError;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct StorageFault(Box<dyn StdError + Send + Sync>);

impl StorageFault {
    pub fn new(source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage is unavailable: {0}")]
    Unavailable(String),

    #[error(transparent)]
    Fault(#[from] StorageFault),
}

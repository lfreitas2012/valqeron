use thiserror::Error;
use crate::StorageFault;

#[derive(Debug, Error)]
pub enum BackgroundTasksError {
    #[error(transparent)]
    Storage(#[from] StorageFault),
}
use crate::StorageFault;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackgroundTasksError {
    #[error(transparent)]
    Storage(#[from] StorageFault),
}
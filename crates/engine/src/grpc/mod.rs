mod admin;
mod issuer;

use crate::storage::AsyncStorage;
use tonic::Status;
use valqeron_core::Repositories;
use valqeron_infrastructure::SqliteStorageEngine;

#[derive(Clone)]
pub(crate) struct ValqeronEngineGrpc {
    storage: AsyncStorage,
}

impl ValqeronEngineGrpc {
    pub fn new(storage: AsyncStorage) -> Self {
        Self { storage }
    }

    async fn run_read<T, E, F>(&self, operation: &'static str, f: F) -> Result<T, Status>
    where
        F: FnOnce(&Repositories<SqliteStorageEngine>) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
        Status: From<E>,
    {
        self.storage
            .read(operation, f)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(Status::from)
    }

    async fn run_write<T, E, F>(
        &self,
        operation: &'static str,
        dry_run: bool,
        f: F,
    ) -> Result<T, Status>
    where
        F: FnOnce(&Repositories<SqliteStorageEngine>) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
        Status: From<E>,
        E: From<valqeron_core::StorageError>,
    {
        self.storage
            .write(operation, dry_run, f)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(Status::from)
    }
}

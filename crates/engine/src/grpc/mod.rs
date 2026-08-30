mod admin;
mod client;
mod issuer;

use crate::storage::AsyncStorage;
use tonic::Status;
use valqeron_core::Repositories;
use valqeron_infrastructure::SqliteStorageEngine;

pub mod v1 {
    tonic::include_proto!("valqeron.v1");
}

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone)]
pub(crate) struct ValqeronEngineGrpc {
    storage: AsyncStorage,
}

impl ValqeronEngineGrpc {
    pub fn new(storage: AsyncStorage) -> Self {
        Self { storage }
    }

    async fn run_read<T, F>(&self, operation: &'static str, f: F) -> Result<T, Status>
    where
        F: FnOnce(&Repositories<SqliteStorageEngine>) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.storage
            .read(operation, f)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(|e| Status::internal(e.to_string()))
    }

    async fn run_write<T, F>(
        &self,
        operation: &'static str,
        dry_run: bool,
        f: F,
    ) -> Result<T, Status>
    where
        F: FnOnce(&Repositories<SqliteStorageEngine>) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.storage
            .write(operation, dry_run, f)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(|e| Status::internal(e.to_string()))
    }
}

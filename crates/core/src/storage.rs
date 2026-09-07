mod error;

use crate::domain::issuer::IssuerRepository;
use crate::domain::security::SecurityRepository;
use crate::tasks::BackgroundTasksRegistryRepository;
pub use error::{StorageError, StorageFault};

pub struct Repositories<E: StorageEngine> {
    pub issuers: E::Issuers,
    pub securities: E::Securities,
    pub background_task_definition: E::BackgroundTaskDefinition,
}

pub trait StorageEngine: Sized + Send + Sync {
    type Issuers: IssuerRepository;
    type Securities: SecurityRepository;
    type BackgroundTaskDefinition: BackgroundTasksRegistryRepository;

    fn repositories(&self) -> Repositories<Self>;

    /// # Errors
    ///
    /// Wil return `StorageError` if the underlying storage engine is unavailable or a storage fault
    /// occurs.
    fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Repositories<Self>) -> T;
}

pub struct PersistenceManager<E: StorageEngine> {
    engine: E,
}

impl<E: StorageEngine> PersistenceManager<E> {
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    pub fn repositories(&self) -> Repositories<E> {
        self.engine.repositories()
    }

    /// # Errors
    ///
    /// Wil return `StorageError` if the underlying storage engine is unavailable or a storage fault
    /// occurs.
    pub fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Repositories<E>) -> T,
    {
        self.engine.dry_run(f)
    }
}

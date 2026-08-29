mod error;

use crate::domain::issuer::IssuerRepository;
use crate::domain::security::SecurityRepository;
pub use error::{StorageError, StorageFault};

pub struct Repositories<E: StorageEngine> {
    pub issuers: E::Issuers,
    pub securities: E::Securities,
    pub background_task: E::BackgroundTasks,
}

pub trait StorageEngine: Sized + Send + Sync {
    type Issuers: IssuerRepository;
    type Securities: SecurityRepository;
    type BackgroundTasks: BackgroundTasksRepository;

    fn repositories(&self) -> Repositories<Self>;

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

    pub fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Repositories<E>) -> T,
    {
        self.engine.dry_run(f)
    }
}

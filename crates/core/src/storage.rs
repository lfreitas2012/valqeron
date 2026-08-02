use crate::issuer::repository::IssuerRepository;

mod error;

pub use error::{StorageError, StorageFault};

/// The set of repositories a [`StorageEngine`] exposes.
///
/// Each engine declares the concrete repository types it provides; the domain and application
/// layers reach persistence only through these ports, never through a concrete driver.
pub struct Repositories<E: StorageEngine> {
    pub issuers: E::Issuers,
}

/// A persistence backend (the driver-facing port).
///
/// Implementors are concrete drivers (SQLite today) living in the infrastructure layer. The domain
/// speaks only in terms of this trait and the repository ports it exposes, so the vocabulary here is
/// deliberately backend-agnostic: it names no SQL, pool, or pragma concepts.
pub trait StorageEngine: Sized + Send + Sync {
    type Issuers: IssuerRepository;

    fn repositories(&self) -> Repositories<Self>;

    /// Run `f` against a dry-run view of the store: every write it performs is rolled back on
    /// return and never persisted.
    ///
    /// Store-level failures (opening the transaction, rolling it back) surface as [`StorageError`];
    /// the closure's own value is returned unchanged on success.
    fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Repositories<Self>) -> T;
}

/// The application-facing handle over a [`StorageEngine`].
///
/// Thin generic wrapper the application layer holds; it delegates to the underlying engine and
/// keeps callers free of any concrete driver type.
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

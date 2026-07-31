//! Backend-agnostic persistence abstraction.
//!
//! [`Store`] is the facade applications use to reach persistence. It owns a
//! [`StorageBackend`] trait object, so the concrete storage engine (SQLite today,
//! others later) is chosen at construction time and hidden from callers behind
//! the domain repository traits.

use crate::issuer::repository::IssuerRepository;

/// Durability level for committed writes.
///
/// Driver-neutral knob mapped by each backend onto its native setting (for SQLite, the
/// `synchronous` pragma).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Durability {
    /// Favor throughput/latency. Committed transactions can be lost on a power/OS crash, but the
    /// database is never corrupted. Suitable for the embedded single-app use case.
    #[default]
    Relaxed,
    /// Favor durability. Committed transactions survive a power loss, at the cost of write latency.
    Strict,
}

/// Driver-agnostic configuration supplied when opening a backend.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Number of concurrent read operations the backend can serve without
    /// queuing. Higher values help read-heavy multithreaded UIs. Backends that
    /// do not pool readers may ignore this.
    pub reader_pool_size: usize,

    /// Durability level for committed writes (see [`Durability`]). Defaults to
    /// [`Durability::Relaxed`].
    pub durability: Durability,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            reader_pool_size: 4,
            durability: Durability::default(),
        }
    }
}

/// Errors from opening, migrating, or running against a storage backend.
///
/// These are intentionally driver-neutral: a backend maps its own error types
/// onto these variants so callers never depend on a concrete engine.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The storage could not be opened or configured.
    #[error("failed to open storage")]
    Open {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A schema migration failed to apply.
    #[error("failed to apply schema migrations")]
    Migration {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A dry-run's isolated context could not be started.
    #[error("failed to start dry-run")]
    DryRun {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The on-disk schema is newer than this build understands.
    #[error(
        "the storage schema (version {found}) is newer than this build supports (knows {known}) — upgrade the application"
    )]
    SchemaTooNew { found: i64, known: usize },

    /// The supplied configuration is invalid.
    #[error("invalid storage configuration: {0}")]
    Config(String),

    /// A backend-specific failure that does not map onto a more specific variant.
    #[error(transparent)]
    Backend(#[from] anyhow::Error),
}

/// A pluggable persistence engine.
///
/// Implementors own whatever connections/pools they need and expose the domain
/// repositories plus lifecycle operations. The trait is deliberately
/// object-safe so [`Store`] can hold a `Box<dyn StorageBackend>` and select the
/// engine at runtime.
pub trait StorageBackend: Send + Sync {
    /// A ready-to-use issuer repository. Cheap to call.
    fn issuers(&self) -> Box<dyn IssuerRepository>;

    /// Apply any pending schema migrations. Idempotent.
    fn migrate(&self) -> Result<(), StorageError>;

    /// Run `f` against an isolated dry-run view of the repositories. Every write
    /// performed inside `f` is rolled back on return and never persisted.
    ///
    /// Object-safe by design: it takes `&mut dyn FnMut` rather than a generic
    /// closure. [`Store::dry_run`] provides ergonomic, value-returning sugar
    /// over this method.
    fn dry_run(&self, f: &mut dyn FnMut(&dyn IssuerRepository)) -> Result<(), StorageError>;
}

/// The persistence facade. Construct once at startup with a concrete backend,
/// then hand out repositories for domain work.
pub struct Store {
    backend: Box<dyn StorageBackend>,
}

impl Store {
    /// Wrap a concrete backend behind the facade.
    pub fn new(backend: Box<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// A ready-to-use issuer repository.
    pub fn issuers(&self) -> Box<dyn IssuerRepository> {
        self.backend.issuers()
    }

    /// Apply any pending schema migrations.
    pub fn migrate(&self) -> Result<(), StorageError> {
        self.backend.migrate()
    }

    /// Run `f` against an isolated dry-run view of the repositories, returning
    /// the closure's value. Every write performed inside `f` is rolled back and
    /// never persisted.
    ///
    /// This is generic sugar over [`StorageBackend::dry_run`]: the return value
    /// is captured internally so callers get normal ergonomics despite the
    /// object-safe backend contract.
    pub fn dry_run<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&dyn IssuerRepository) -> T,
    {
        let mut f = Some(f);
        let mut out: Option<T> = None;
        self.backend.dry_run(&mut |repo| {
            if let Some(f) = f.take() {
                out = Some(f(repo));
            }
        })?;
        Ok(out.expect("dry_run closure must be invoked exactly once by the backend"))
    }
}

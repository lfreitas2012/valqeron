//! The core engine: the single entry point applications use to initialize storage and get repositories.
//!
//! The engine hides the storage driver entirely. Callers never see connections, pools, or the
//! concrete SQLite repository, only [`Engine`] and the [`IssuerRepository`](IssuerRepository) trait.

use std::path::Path;

use crate::db::{Database, DatabaseConfig, SqliteDataDriverError};
use crate::issuer::repository::IssuerRepository;
use crate::issuer::repository::sqlite::SqliteIssuerRepository;

/// Engine configuration. Driver-agnostic knobs supplied at open time.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Number of concurrent read operations the engine can serve without queuing. Higher values
    /// help read-heavy multithreaded UIs.
    pub reader_pool_size: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            reader_pool_size: DatabaseConfig::default().reader_pool_size,
        }
    }
}

impl From<EngineConfig> for DatabaseConfig {
    fn from(cfg: EngineConfig) -> Self {
        DatabaseConfig {
            reader_pool_size: cfg.reader_pool_size,
        }
    }
}

/// Errors from initializing the engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
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

    /// The on-disk schema is newer than this build understands.
    #[error(
        "the database schema (version {found}) is newer than this build supports (knows {known}) — upgrade the application"
    )]
    SchemaTooNew { found: i64, known: usize },

    /// The supplied configuration is invalid.
    #[error("invalid engine configuration: {0}")]
    Config(String),
}

impl From<SqliteDataDriverError> for EngineError {
    fn from(e: SqliteDataDriverError) -> Self {
        match e {
            SqliteDataDriverError::Connection { source }
            | SqliteDataDriverError::Pragma { source } => EngineError::Open {
                source: Box::new(source),
            },
            SqliteDataDriverError::DryRun { source } => EngineError::Open {
                source: Box::new(source),
            },
            SqliteDataDriverError::Migration { source } => EngineError::Migration { source },
            SqliteDataDriverError::UnknownSchemaVersion { found, known } => {
                EngineError::SchemaTooNew { found, known }
            }
            SqliteDataDriverError::InvalidPoolSize => {
                EngineError::Config("reader pool size must be at least 1".into())
            }
        }
    }
}

/// The initialized core. Construct once at application startup, then hand out repositories for domain work.
pub struct Engine {
    db: Database,
}

impl Engine {
    /// Open (or create) the engine's storage at `path`, applying any pending schema migrations.
    /// Uses default configuration.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        Self::open_with_config(path, EngineConfig::default())
    }

    /// Open (or create) the engine's storage at `path` with explicit configuration.
    pub fn open_with_config(
        path: impl AsRef<Path>,
        config: EngineConfig,
    ) -> Result<Self, EngineError> {
        let db = Database::open_with_config(path, config.into())?;
        Ok(Self { db })
    }

    /// A ready-to-use issuer repository. Cheap to call; the returned value borrows the engine.
    pub fn issuers(&self) -> impl IssuerRepository + '_ {
        SqliteIssuerRepository::new(self.db.handle())
    }

    /// Run `f` against an isolated dry-run view of the repositories. Every write performed inside
    /// `f` is rolled back on return and never persisted, and concurrent work on other threads is
    /// unaffected.
    pub fn dry_run<F, T>(&self, f: F) -> Result<T, EngineError>
    where
        F: FnOnce(&dyn IssuerRepository) -> T,
    {
        let result = self.db.dry_run(|handle| {
            let repo = SqliteIssuerRepository::new(handle.clone());
            f(&repo)
        })?;
        Ok(result)
    }
}

use valqeron_core::{StorageError, StorageFault};

#[derive(Debug, thiserror::Error)]
pub enum SqliteDbError {
    #[error("failed to open sqlite connection")]
    Connection {
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to configure connection")]
    Pragma {
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to apply schema migrations")]
    Migration {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error(
        "database schema version {found} is newer than the {known} migration(s) this binary knows about — upgrade the binary"
    )]
    UnknownSchemaVersion { found: i64, known: usize },

    #[error("reader pool size must be at least 1")]
    InvalidPoolSize,
}

/// Translate a driver error into the domain's opaque storage fault, preserving the source chain.
impl From<SqliteDbError> for StorageFault {
    fn from(err: SqliteDbError) -> Self {
        StorageFault::new(err)
    }
}

/// Translate a driver error into the domain's store-level error.
impl From<SqliteDbError> for StorageError {
    fn from(err: SqliteDbError) -> Self {
        StorageError::Fault(StorageFault::new(err))
    }
}

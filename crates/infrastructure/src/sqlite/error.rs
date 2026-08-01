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

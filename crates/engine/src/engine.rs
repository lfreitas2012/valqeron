mod error;
mod lockfile;

use crate::engine::error::EngineError;
use directories::ProjectDirs;
use std::path::PathBuf;

// ================ TYPES ================
pub type EngineResult<T> = Result<T, EngineError>;

// ================ ENVIRONMENT VARIABLES ================
/// Engine socket path environment variable.
pub const ENGINE_SOCKET_PATH_ENV: &str = "VALQERON_ENGINE_SOCKET_PATH";
/// Engine database path environment variable.
pub const DB_PATH_ENV: &str = "VALQERON_ENGINE_DB_PATH";

// ================ DEFAULT VALUES ================
pub const DEFAULT_ENGINE_SOCKET_PATH: &str = "/run/valqeron-engine.sock";
pub const DEFAULT_VALQERON_QUALIFIER: &str = "io";
pub const DEFAULT_VALQERON_ORGANIZATION: &str = "valqeron";
pub const DEFAULT_VALQERON_APP: &str = "valqeron";
pub const DEFAULT_ENGINE_DB_NAME: &str = "valqeron.db";

// ================ DATABASE PATH ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DatabasePath(PathBuf);

impl DatabasePath {
    pub fn resolve(override_path: Option<PathBuf>) -> EngineResult<Self> {
        let project_data_dir = ProjectDirs::from(
            DEFAULT_VALQERON_QUALIFIER,
            DEFAULT_VALQERON_ORGANIZATION,
            DEFAULT_VALQERON_APP,
        )
        .map(|dirs| dirs.data_dir().to_path_buf());

        Self::resolve_with_project_data_dir(override_path, project_data_dir)
    }

    fn resolve_with_project_data_dir(
        override_path: Option<PathBuf>,
        project_data_dir: Option<PathBuf>,
    ) -> EngineResult<Self> {
        if let Some(path) = override_path {
            return Ok(Self(path));
        }

        if let Some(path) = std::env::var_os(DB_PATH_ENV) {
            return Ok(Self(PathBuf::from(path)));
        }

        project_data_dir
            .map(|dir| Self(dir.join(DEFAULT_ENGINE_DB_NAME)))
            .ok_or(EngineError::InvalidDatabasePath {
                app: DEFAULT_VALQERON_APP,
                env_var: DB_PATH_ENV,
            })
    }
}

impl From<DatabasePath> for PathBuf {
    fn from(path: DatabasePath) -> Self {
        path.0
    }
}

// ================ SOCKET PATH ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    pub fn resolve(override_path: Option<PathBuf>) -> Self {
        if let Some(path) = override_path {
            return Self(path);
        }

        if let Some(path) = std::env::var_os(ENGINE_SOCKET_PATH_ENV) {
            return Self(PathBuf::from(path));
        }

        Self(PathBuf::from(DEFAULT_ENGINE_SOCKET_PATH))
    }
}

impl From<SocketPath> for PathBuf {
    fn from(path: SocketPath) -> Self {
        path.0
    }
}

// ================ ENGINE CONFIGURATION ================
#[derive(Debug, Clone)]
pub struct EngineConfig {
    db_path: DatabasePath,
    socket_path: SocketPath,
}

impl EngineConfig {
    pub fn builder() -> EngineConfigBuilder {
        EngineConfigBuilder::new()
    }
}

#[derive(Default)]
pub struct EngineConfigBuilder {
    db_path: Option<DatabasePath>,
    socket_path: Option<SocketPath>,
}

impl EngineConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn db_path(mut self, db_path: impl Into<DatabasePath>) -> Self {
        self.db_path = Some(db_path.into());
        self
    }

    pub fn socket_path(mut self, socket_path: impl Into<SocketPath>) -> Self {
        self.socket_path = Some(socket_path.into());
        self
    }

    pub fn build(self) -> Result<EngineConfig, EngineError> {
        let db_path = match self.db_path {
            Some(path) => path,
            None => DatabasePath::resolve(None)?,
        };

        let socket_path = self
            .socket_path
            .unwrap_or_else(|| SocketPath::resolve(None));

        Ok(EngineConfig {
            db_path,
            socket_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_config_builder_resolves_defaults() {
        let config = EngineConfigBuilder::new().build().unwrap();

        assert!(Some(config.db_path).is_some());
        assert_eq!(
            config.socket_path.0,
            PathBuf::from(DEFAULT_ENGINE_SOCKET_PATH)
        );
    }

    #[test]
    fn test_database_path_resolve_errors_when_no_override_env_or_project_dir() {
        unsafe {
            std::env::remove_var(DB_PATH_ENV);
        }

        let result = DatabasePath::resolve_with_project_data_dir(None, None);

        assert!(matches!(
            result,
            Err(EngineError::InvalidDatabasePath {
                app: DEFAULT_VALQERON_APP,
                env_var: DB_PATH_ENV,
            })
        ));
    }
}

//! Path and environment resolution for the engine (absorbed from the former
//! `valqeron-config` crate).
//!
//! The engine owns the database, so database-path resolution is an
//! engine-internal concern: clients never resolve a DB path — they resolve
//! the engine *socket* through [`valqeron_proto::connection`]. Keep the two
//! in the same mental bucket: `VALQERON_DB` steers the engine, and
//! `VALQERON_SOCKET` steers everyone.
//!
//! Precedence everywhere: explicit flag > environment variable > platform
//! default (via `directories::ProjectDirs`).

use std::ffi::OsStr;
use std::path::PathBuf;

use directories::ProjectDirs;

/// `ProjectDirs` qualifier shared by every Valqeron application.
const QUALIFIER: &str = "io";
/// `ProjectDirs` organization shared by every Valqeron application.
const ORGANIZATION: &str = "valqeron";
/// Application name for state shared by all binaries (the database).
const SHARED_APP: &str = "valqeron";
/// Application name for engine-private files (logs).
const ENGINE_APP: &str = "valqeron-engine";

/// Default database file name inside the shared data directory.
pub const DB_FILE_NAME: &str = "valqeron.db";
/// Environment variable overriding the database path.
pub const DB_PATH_ENV: &str = "VALQERON_DB";

/// Environment variable naming the engine log file (or `off` to disable).
pub const LOG_FILE_ENV: &str = "VALQERON_ENGINE_LOG_FILE";
/// Environment variable overriding the engine file log level.
pub const LOG_LEVEL_ENV: &str = "VALQERON_ENGINE_LOG_LEVEL";
/// Default engine log file name.
const LOG_FILE_NAME: &str = "valqeron-engine.log";
/// Default level for the JSON file log layer when the env override is absent.
const DEFAULT_FILE_LOG_LEVEL: &str = "info";

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("could not determine a home directory for {purpose}")]
    NoHomeDirectory { purpose: &'static str },
}

fn shared_dirs() -> Result<ProjectDirs, PathError> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, SHARED_APP).ok_or(PathError::NoHomeDirectory {
        purpose: "the database",
    })
}

fn engine_dirs() -> Result<ProjectDirs, PathError> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, ENGINE_APP).ok_or(PathError::NoHomeDirectory {
        purpose: "engine files",
    })
}

/// Resolve the SQLite database path: flag > `VALQERON_DB` > shared data dir.
pub fn resolve_db_path(flag: Option<PathBuf>) -> Result<PathBuf, PathError> {
    if let Some(path) = flag {
        return Ok(path);
    }
    if let Some(env) = std::env::var_os(DB_PATH_ENV) {
        return Ok(PathBuf::from(env));
    }
    Ok(shared_dirs()?.data_dir().join(DB_FILE_NAME))
}

/// Resolve the engine log file location.
///
/// - `no_log_file` (`--no-log-file`) always wins and disables file logging.
/// - `flag = Some(Some(path))` (`--log-file PATH`) pins an explicit path.
/// - `flag = Some(None)` (bare `--log-file`) selects the default location.
/// - `flag = None` consults `VALQERON_ENGINE_LOG_FILE`; the values `off`,
///   `false`, `0` and `none` (case-insensitive) disable file logging; any
///   other value is used as the path. Absent env means file logging is on.
pub fn resolve_log_file(
    flag: Option<Option<PathBuf>>,
    no_log_file: bool,
) -> Result<Option<PathBuf>, PathError> {
    if no_log_file {
        return Ok(None);
    }

    match flag {
        Some(Some(path)) => Ok(Some(path)),
        Some(None) => Ok(Some(default_log_file()?)),
        None => match std::env::var_os(LOG_FILE_ENV) {
            Some(env) if is_off(&env) => Ok(None),
            Some(env) => Ok(Some(PathBuf::from(env))),
            None => Ok(Some(default_log_file()?)),
        },
    }
}

/// Default engine log file: the platform state dir when it exists,
/// otherwise `<data dir>/logs`.
pub fn default_log_file() -> Result<PathBuf, PathError> {
    let dirs = engine_dirs()?;
    let dir = dirs
        .state_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs.data_dir().join("logs"));
    Ok(dir.join(LOG_FILE_NAME))
}

/// File log level: `VALQERON_ENGINE_LOG_LEVEL`, or the `info` default.
pub fn file_log_level() -> String {
    std::env::var(LOG_LEVEL_ENV).unwrap_or_else(|_| DEFAULT_FILE_LOG_LEVEL.to_string())
}

fn is_off(value: &OsStr) -> bool {
    value
        .to_str()
        .map(|s| {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "off" | "false" | "0" | "none"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn explicit_db_flag_wins_over_everything() {
        let path = resolve_db_path(Some(PathBuf::from("/tmp/explicit.db"))).unwrap();
        assert_eq!(path, Path::new("/tmp/explicit.db"));
    }

    #[test]
    fn default_db_path_lives_in_shared_data_dir() {
        // The env var may leak from the calling shell, so only assert the
        // default shape when it is absent.
        if std::env::var_os(DB_PATH_ENV).is_none() {
            let path = resolve_db_path(None).unwrap();
            assert!(path.ends_with(DB_FILE_NAME), "unexpected default: {path:?}");
        }
    }

    #[test]
    fn explicit_log_file_flag_wins() {
        let log = resolve_log_file(Some(Some(PathBuf::from("/tmp/out.log"))), false).unwrap();
        assert_eq!(log, Some(PathBuf::from("/tmp/out.log")));
    }

    #[test]
    fn log_file_flag_without_path_uses_default_location() {
        let log = resolve_log_file(Some(None), false)
            .unwrap()
            .expect("default log path");
        assert!(log.ends_with(LOG_FILE_NAME));
    }

    #[test]
    fn no_log_file_beats_explicit_path() {
        let log = resolve_log_file(Some(Some(PathBuf::from("/tmp/out.log"))), true).unwrap();
        assert!(log.is_none());
    }

    #[test]
    fn is_off_recognizes_disable_values() {
        for v in ["off", "OFF", "Off", "false", "0", "none", "  off  "] {
            assert!(is_off(OsStr::new(v)), "{v:?} should disable");
        }
        for v in ["on", "1", "true", "/tmp/logs/x.log"] {
            assert!(!is_off(OsStr::new(v)), "{v:?} should not disable");
        }
    }
}

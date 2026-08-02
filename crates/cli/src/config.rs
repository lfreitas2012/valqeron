use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use valqeron_infrastructure::Synchronous;

use crate::error::{AppError, AppResult};

const QUALIFIER: &str = "io";
const ORGANIZATION: &str = "valqeron";
const SHARED_APP: &str = "valqeron";
const CLI_APP: &str = "valqeron-cli";

const DB_FILE_NAME: &str = "valqeron.db";
const LOG_FILE_NAME: &str = "valqeron-cli.log";

const DEFAULT_FILE_LOG_LEVEL: &str = "info";

#[derive(Debug, Clone)]
pub struct ValqeronConfig {
    db_path: PathBuf,
    log_file: Option<PathBuf>,
    reader_pool_size: usize,
    durable: bool,
}

impl ValqeronConfig {
    pub fn resolve(
        db_path_flag: Option<PathBuf>,
        log_file_flag: Option<Option<PathBuf>>,
        no_log_file: bool,
        reader_pool_size: usize,
        durable: bool,
    ) -> AppResult<Self> {
        let db_path = resolve_db_path(db_path_flag)?;
        let log_file = resolve_log_file(log_file_flag, no_log_file)?;
        Ok(Self {
            db_path,
            log_file,
            reader_pool_size,
            durable,
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }

    pub fn file_log_level(&self) -> String {
        std::env::var("VALQERON_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_FILE_LOG_LEVEL.to_string())
    }

    pub fn reader_pool_size(&self) -> usize {
        self.reader_pool_size
    }

    pub fn synchronous(&self) -> Synchronous {
        if self.durable {
            Synchronous::Full
        } else {
            Synchronous::Normal
        }
    }

    pub fn durability_label(&self) -> &'static str {
        if self.durable { "strict" } else { "relaxed" }
    }

    pub fn ensure_db_parent(&self) -> AppResult<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(format!("creating {}: {e}", parent.display())))?;
        }
        Ok(())
    }
}

fn shared_dirs() -> AppResult<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, SHARED_APP).ok_or_else(|| {
        AppError::Config("could not determine a home directory for the database".into())
    })
}

fn cli_dirs() -> AppResult<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, CLI_APP)
        .ok_or_else(|| AppError::Config("could not determine a home directory for logs".into()))
}

fn resolve_db_path(flag: Option<PathBuf>) -> AppResult<PathBuf> {
    if let Some(path) = flag {
        return Ok(path);
    }
    if let Some(env) = std::env::var_os("VALQERON_DB") {
        return Ok(PathBuf::from(env));
    }
    Ok(shared_dirs()?.data_dir().join(DB_FILE_NAME))
}

fn resolve_log_file(
    flag: Option<Option<PathBuf>>,
    no_log_file: bool,
) -> AppResult<Option<PathBuf>> {
    // Explicit opt-out always wins.
    if no_log_file {
        return Ok(None);
    }

    match flag {
        // `--log-file <PATH>` explicitly pins a path.
        Some(Some(path)) => Ok(Some(path)),
        // `--log-file` with no value → default location.
        Some(None) => Ok(Some(default_log_file()?)),
        // Flag absent → consult env; otherwise file logging is on by default.
        None => match std::env::var_os("VALQERON_LOG_FILE") {
            // `VALQERON_LOG_FILE=off` (case-insensitive) disables file logging.
            Some(env) if is_off(&env) => Ok(None),
            Some(env) => Ok(Some(PathBuf::from(env))),
            None => Ok(Some(default_log_file()?)),
        },
    }
}

fn is_off(value: &std::ffi::OsStr) -> bool {
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

fn default_log_file() -> AppResult<PathBuf> {
    let dirs = cli_dirs()?;
    let dir = dirs
        .state_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs.data_dir().join("logs"));
    Ok(dir.join(LOG_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_db_flag_wins() {
        let cfg = ValqeronConfig::resolve(Some(PathBuf::from("/tmp/x.db")), None, false, 4, false)
            .unwrap();
        assert_eq!(cfg.db_path(), Path::new("/tmp/x.db"));
        assert_eq!(cfg.reader_pool_size(), 4);
        assert_eq!(cfg.synchronous(), Synchronous::Normal);
    }

    #[test]
    fn explicit_log_file_flag_wins() {
        let cfg = ValqeronConfig::resolve(
            Some(PathBuf::from("/tmp/x.db")),
            Some(Some(PathBuf::from("/tmp/out.log"))),
            false,
            2,
            false,
        )
        .unwrap();
        assert_eq!(cfg.log_file(), Some(Path::new("/tmp/out.log")));
    }

    #[test]
    fn log_file_flag_without_path_uses_default_location() {
        let cfg = ValqeronConfig::resolve(
            Some(PathBuf::from("/tmp/x.db")),
            Some(None),
            false,
            4,
            false,
        )
        .unwrap();
        let log = cfg.log_file().expect("default log path");
        assert!(log.ends_with(LOG_FILE_NAME));
    }

    #[test]
    fn file_logging_is_on_by_default_when_flag_absent() {
        // Absent flag (and no opt-out) resolves to the default log location.
        let cfg = ValqeronConfig::resolve(Some(PathBuf::from("/tmp/x.db")), None, false, 4, false)
            .unwrap();
        let log = cfg.log_file().expect("default log path when flag absent");
        assert!(log.ends_with(LOG_FILE_NAME));
    }

    #[test]
    fn no_log_file_flag_disables_file_logging() {
        // `--no-log-file` wins even over an explicit `--log-file PATH`.
        let cfg = ValqeronConfig::resolve(
            Some(PathBuf::from("/tmp/x.db")),
            Some(Some(PathBuf::from("/tmp/out.log"))),
            true,
            4,
            false,
        )
        .unwrap();
        assert!(cfg.log_file().is_none());
    }

    #[test]
    fn durable_flag_selects_strict_durability() {
        let cfg = ValqeronConfig::resolve(Some(PathBuf::from("/tmp/x.db")), None, false, 4, true)
            .unwrap();
        assert_eq!(cfg.synchronous(), Synchronous::Full);
    }

    #[test]
    fn is_off_recognizes_disable_values() {
        use std::ffi::OsStr;
        for v in ["off", "OFF", "Off", "false", "0", "none", "  off  "] {
            assert!(is_off(OsStr::new(v)), "{v:?} should disable");
        }
        for v in ["on", "1", "true", "/tmp/logs/x.log"] {
            assert!(!is_off(OsStr::new(v)), "{v:?} should not disable");
        }
    }
}

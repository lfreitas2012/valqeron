//! Runtime configuration and on-disk path resolution.
//!
//! # Directory strategy
//!
//! Valqeron is a family of front-ends (this CLI today; a desktop app and a
//! daemon later) that all operate on the **same** issuer database. To make that
//! work out of the box while keeping each binary's diagnostics separate, paths
//! are split across two [`directories::ProjectDirs`] roots:
//!
//! * **Shared product dir** — `io.valqeron.valqeron` — holds the **database**,
//!   so every Valqeron front-end defaults to the same file.
//! * **Per-binary dir** — `io.valqeron.valqeron-cli` — holds this binary's
//!   **logs** (and, later, CLI-specific config), so they never intermix with
//!   the app's or the daemon's.
//!
//! # Precedence
//!
//! Each resource resolves independently as: explicit flag → environment
//! variable → platform default.
//!
//! | Resource | Flag           | Env                 | Default (root)         |
//! |----------|----------------|---------------------|------------------------|
//! | Database | `--db-path`    | `VALQERON_DB`       | shared product data    |
//! | Log file | `--log-file`   | `VALQERON_LOG_FILE` | per-binary data/logs   |
//!
//! # Logging
//!
//! File logging is **on by default**: every run appends structured logs to the
//! per-binary logs directory so all operations are recorded, independent of the
//! `-v` (stderr) verbosity. It can be disabled with `--no-log-file` or by
//! setting `VALQERON_LOG_FILE=off`. The file's own level defaults to `info`
//! (overridable via `VALQERON_LOG_LEVEL`), so the audit trail is complete even
//! when stderr stays quiet.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use valqeron_infrastructure::Synchronous;

use crate::error::{AppError, AppResult};

const QUALIFIER: &str = "io";
const ORGANIZATION: &str = "valqeron";
/// Shared across all Valqeron front-ends → shared database location.
const SHARED_APP: &str = "valqeron";
/// Unique to this binary → isolated logs / config location.
const CLI_APP: &str = "valqeron-cli";

const DB_FILE_NAME: &str = "valqeron.db";
const LOG_FILE_NAME: &str = "valqeron-cli.log";

/// Default level for the file log layer (captures all operations).
const DEFAULT_FILE_LOG_LEVEL: &str = "info";

/// Fully resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct ValqeronConfig {
    db_path: PathBuf,
    log_file: Option<PathBuf>,
    reader_pool_size: usize,
    durable: bool,
}

impl ValqeronConfig {
    /// Resolve configuration from the parsed CLI values.
    ///
    /// * `db_path_flag` — value of `--db-path`, if provided.
    /// * `log_file_flag` — `Some(None)` means `--log-file` was passed with no
    ///   path (use the default location); `Some(Some(p))` pins a path; `None`
    ///   means the flag was absent (default location, unless disabled).
    /// * `no_log_file` — value of `--no-log-file`; when `true`, file logging is
    ///   disabled regardless of the other inputs.
    /// * `reader_pool_size` — value of `--reader-pool-size`.
    /// * `durable` — value of `--durable`; when `true`, the writer uses the
    ///   strict (power-loss-safe) durability level.
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

    /// The resolved database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// The resolved log file path, if file logging is enabled.
    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }

    /// The level directive for the file log layer.
    ///
    /// Defaults to `info` so the file captures all operations, independent of
    /// the `-v` (stderr) verbosity. Overridable via `VALQERON_LOG_LEVEL`.
    pub fn file_log_level(&self) -> String {
        std::env::var("VALQERON_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_FILE_LOG_LEVEL.to_string())
    }

    /// The configured reader-pool size for the engine.
    pub fn reader_pool_size(&self) -> usize {
        self.reader_pool_size
    }

    /// The configured writer durability level, expressed as the storage backend's `synchronous`
    /// pragma level.
    ///
    /// `--durable` selects [`Synchronous::Full`] (committed writes survive power loss, slower); the
    /// default is [`Synchronous::Normal`] (faster, may lose the last commit on a crash).
    pub fn synchronous(&self) -> Synchronous {
        if self.durable {
            Synchronous::Full
        } else {
            Synchronous::Normal
        }
    }

    /// A human-readable label for the configured durability, for logging/output.
    pub fn durability_label(&self) -> &'static str {
        if self.durable { "Strict" } else { "Relaxed" }
    }

    /// Ensure the parent directory of the database exists, creating it if
    /// necessary. Called before opening the engine (notably by `init`).
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

/// Whether an env value means "disable file logging" (`off`/`false`/`0`/`none`, case-insensitive).
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
    // Prefer a dedicated state dir when the platform has one; otherwise nest a
    // `logs/` subdir under the data dir.
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

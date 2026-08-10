use anyhow::Context;
use directories::ProjectDirs;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const QUALIFIER: &str = "io";
const ORGANIZATION: &str = "valqeron";
const CLI_APP: &str = "valqeron-cli";

/// Environment variable naming the CLI log file (or `off` to disable).
pub const LOG_FILE_ENV: &str = "VALQERON_LOG_FILE";
/// Environment variable overriding the CLI file log level.
pub const LOG_LEVEL_ENV: &str = "VALQERON_LOG_LEVEL";
/// Default CLI log file name.
pub const LOG_FILE_NAME: &str = "valqeron-cli.log";
/// Default level for the JSON file log layer when the env override is absent.
const DEFAULT_FILE_LOG_LEVEL: &str = "info";

#[derive(Debug, Clone)]
pub struct ValqeronConfig {
    log_file: Option<PathBuf>,
}

impl ValqeronConfig {
    pub fn resolve(
        log_file_flag: Option<Option<PathBuf>>,
        no_log_file: bool,
    ) -> anyhow::Result<Self> {
        let log_file = if no_log_file {
            None
        } else {
            match log_file_flag {
                Some(Some(path)) => Some(path),
                Some(None) => Some(default_log_file()?),
                None => match std::env::var_os(LOG_FILE_ENV) {
                    Some(env) if is_off(&env) => None,
                    Some(env) => Some(PathBuf::from(env)),
                    None => Some(default_log_file()?),
                },
            }
        };
        Ok(Self { log_file })
    }

    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }

    pub fn file_log_level(&self) -> String {
        std::env::var(LOG_LEVEL_ENV).unwrap_or_else(|_| DEFAULT_FILE_LOG_LEVEL.to_string())
    }
}

fn default_log_file() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, CLI_APP)
        .context("could not determine a home directory for application files")?;
    let dir = dirs
        .state_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs.data_dir().join("logs"));
    Ok(dir.join(LOG_FILE_NAME))
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
    use super::*;

    #[test]
    fn explicit_log_file_flag_wins() {
        let cfg =
            ValqeronConfig::resolve(Some(Some(PathBuf::from("/tmp/out.log"))), false).unwrap();
        assert_eq!(cfg.log_file(), Some(Path::new("/tmp/out.log")));
    }

    #[test]
    fn log_file_flag_without_path_uses_default_location() {
        let cfg = ValqeronConfig::resolve(Some(None), false).unwrap();
        let log = cfg.log_file().expect("default log path");
        assert!(log.ends_with(LOG_FILE_NAME));
    }

    #[test]
    fn file_logging_is_on_by_default_when_flag_absent() {
        if std::env::var_os(LOG_FILE_ENV).is_none() {
            let cfg = ValqeronConfig::resolve(None, false).unwrap();
            let log = cfg.log_file().expect("default log path when flag absent");
            assert!(log.ends_with(LOG_FILE_NAME));
        }
    }

    #[test]
    fn no_log_file_flag_disables_file_logging() {
        // `--no-log-file` wins even over an explicit `--log-file PATH`.
        let cfg = ValqeronConfig::resolve(Some(Some(PathBuf::from("/tmp/out.log"))), true).unwrap();
        assert!(cfg.log_file().is_none());
    }

    #[test]
    fn is_off_recognizes_disable_values() {
        for v in ["off", "OFF", "false", "0", "none", "  off  "] {
            assert!(is_off(OsStr::new(v)), "{v:?} should disable");
        }
        for v in ["on", "1", "true", "/tmp/logs/x.log"] {
            assert!(!is_off(OsStr::new(v)), "{v:?} should not disable");
        }
    }
}

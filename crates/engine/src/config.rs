use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::{Cli, RunArgs};
use crate::error::{EngineError, EngineResult};
use crate::paths;
use valqeron_infrastructure::Synchronous;

/// SQLite reader pool size. The gRPC edge fans read across this pool; writes serialize on the
/// single writer regardless.
pub const READER_POOL_SIZE: usize = 4;

/// Storage-facade concurrency: enough blocking closures to saturate the reader pool plus the single
/// writer. More slots would only queue on the pool's own `Condvar`; fewer would leave readers idle.
pub const MAX_IN_FLIGHT_STORAGE: usize = READER_POOL_SIZE.saturating_add(1);

#[derive(Debug, Clone)]
pub struct EngineConfig {
    db_path: PathBuf,
    socket_path: PathBuf,
    log_file: Option<PathBuf>,
    durable: bool,
    maintenance_interval: Duration,
    heartbeat_interval: Duration,
}

impl EngineConfig {
    pub fn resolve(cli: &Cli, run: &RunArgs) -> EngineResult<Self> {
        let db_path = resolve_db_path(cli)?;
        let socket_path = resolve_socket_path(cli)?;
        let log_file =
            paths::resolve_log_file(run.log_file_arg(), run.no_log_file).map_err(config_err)?;
        Ok(Self {
            db_path,
            socket_path,
            log_file,
            durable: run.durable,
            maintenance_interval: Duration::from_secs(run.maintenance_interval.max(1)),
            heartbeat_interval: Duration::from_secs(run.heartbeat_interval.max(1)),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn lock_path(&self) -> PathBuf {
        lock_path_for(&self.db_path)
    }

    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
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

    pub fn maintenance_interval(&self) -> Duration {
        self.maintenance_interval
    }

    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    pub fn ensure_db_parent(&self) -> EngineResult<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
        }
        Ok(())
    }

    /// Create the socket's parent directory and restrict it to `0700`.
    /// Directory permissions are the primary local access control for the
    /// engine socket.
    pub fn ensure_socket_dir(&self) -> EngineResult<()> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| EngineError::Io(format!("restricting {}: {e}", parent.display())))?;
        }
        Ok(())
    }
}

/// Resolve the database path (flag > `VALQERON_DB` > shared data dir).
/// Database resolution is engine-internal: clients resolve the socket, not
/// the database.
pub fn resolve_db_path(cli: &Cli) -> EngineResult<PathBuf> {
    paths::resolve_db_path(cli.db_path.clone()).map_err(config_err)
}

/// Resolve the socket path through the shared wire-contract convention so
/// the engine and every client always agree on the endpoint.
pub fn resolve_socket_path(cli: &Cli) -> EngineResult<PathBuf> {
    valqeron_proto::resolve_socket_path(cli.socket.clone())
        .map_err(|e| EngineError::Config(e.to_string()))
}

/// Single-instance lock file location: `<db>.lock` next to the database.
pub fn lock_path_for(db_path: &Path) -> PathBuf {
    let mut os = db_path.as_os_str().to_os_string();
    os.push(".lock");
    PathBuf::from(os)
}

fn config_err(err: paths::PathError) -> EngineError {
    EngineError::Config(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_file_sits_next_to_the_db() {
        assert_eq!(
            lock_path_for(Path::new("/data/valqeron.db")),
            PathBuf::from("/data/valqeron.db.lock")
        );
    }

    #[test]
    fn zero_intervals_are_clamped_to_one_second() {
        use clap::Parser;
        let cli = Cli::try_parse_from([
            "valqeron-engine",
            "--db-path",
            "/tmp/x.db",
            "--socket",
            "/tmp/x.sock",
            "run",
            "--maintenance-interval",
            "0",
            "--heartbeat-interval",
            "0",
            "--no-log-file",
        ])
        .unwrap();
        let crate::cli::Command::Run(run) = &cli.command else {
            panic!("expected run");
        };
        let cfg = EngineConfig::resolve(&cli, run).unwrap();
        assert_eq!(cfg.maintenance_interval(), Duration::from_secs(1));
        assert_eq!(cfg.heartbeat_interval(), Duration::from_secs(1));
        assert_eq!(cfg.socket_path(), Path::new("/tmp/x.sock"));
    }
}

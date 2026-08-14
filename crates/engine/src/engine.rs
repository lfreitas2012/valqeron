use crate::grpc::{AdminGrpc, IssuerGrpc};
use crate::jobs::{JobSet, PeriodicJob};
use crate::storage::AsyncStorage;
use directories::ProjectDirs;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use valqeron_infrastructure::{DatabaseConfig, SqliteStorageEngine, Synchronous};
use valqeron_proto::v1::rpc_admin_service_server::RpcAdminServiceServer;
use valqeron_proto::v1::rpc_issuer_service_server::RpcIssuerServiceServer;

// ================ TYPES ================
pub type EngineResult<T> = Result<T, EngineError>;

// ================ ERRORS ================
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(
        "could not determine a data directory for {app}; set {env_var} or pass --db-path explicitly"
    )]
    InvalidDatabasePath {
        app: &'static str,
        env_var: &'static str,
    },

    #[error("could not acquire lock on {db_path:?}: {pid} is already running")]
    EngineAlreadyRunning {
        db_path: std::path::PathBuf,
        pid: String,
    },

    #[error("could not open database: {0}")]
    Io(String),

    #[error(transparent)]
    StorageError(#[from] valqeron_core::StorageError),

    #[error("force shutdown requested by user.")]
    ForcedShutdown,
}

// ================ ENVIRONMENT VARIABLES ================
/// Engine socket path environment variable.
pub const ENGINE_SOCKET_PATH_ENV: &str = "VALQERON_ENGINE_SOCKET_PATH";
/// Engine database path environment variable.
pub const DB_PATH_ENV: &str = "VALQERON_ENGINE_DB_PATH";
/// Engine log file path environment variable.
pub const ENGINE_LOG_FILE_ENV: &str = "VALQERON_ENGINE_LOG_FILE";

// ================ DEFAULT VALUES ================
pub const DEFAULT_ENGINE_SOCKET_PATH: &str = "/run/valqeron-engine.sock";
pub const DEFAULT_VALQERON_QUALIFIER: &str = "io";
pub const DEFAULT_VALQERON_ORGANIZATION: &str = "valqeron";
pub const DEFAULT_VALQERON_APP: &str = "valqeron";
pub const DEFAULT_ENGINE_DB_NAME: &str = "valqeron.db";
pub const DEFAULT_ENGINE_LOG_FILE_NAME: &str = "engine.log";

// ================ DATABASE PATH ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DatabasePath(PathBuf);

impl DatabasePath {
    pub fn resolve(override_path: Option<PathBuf>) -> EngineResult<Self> {
        let project_data_dir =
            resolve_default_project_dir().map(|dirs| dirs.data_dir().to_path_buf());

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

impl Display for SocketPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

// ================ ENGINE LOG FILE PATH ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct LogFilePath(PathBuf);

impl LogFilePath {
    pub fn resolve(override_path: Option<PathBuf>) -> EngineResult<Self> {
        let log_file_dir = resolve_default_project_dir().map(|dirs| dirs.data_dir().to_path_buf());

        Self::resolve_with_project_log_file(override_path, log_file_dir)
    }

    fn resolve_with_project_log_file(
        override_path: Option<PathBuf>,
        project_log_file_dir: Option<PathBuf>,
    ) -> EngineResult<Self> {
        if let Some(path) = override_path {
            return Ok(Self(path));
        }

        if let Some(path) = std::env::var_os(ENGINE_LOG_FILE_ENV) {
            return Ok(Self(PathBuf::from(path)));
        }

        project_log_file_dir
            .map(|dir| Self(dir.join(DEFAULT_ENGINE_LOG_FILE_NAME)))
            .ok_or(EngineError::InvalidDatabasePath {
                app: DEFAULT_VALQERON_APP,
                env_var: ENGINE_LOG_FILE_ENV,
            })
    }
}

// ================ ENGINE CONFIGURATION ================
#[derive(Clone, Debug)]
pub struct EngineConfig {
    db_path: DatabasePath,
    socket_path: SocketPath,
    log_file_path: LogFilePath,
}

impl EngineConfig {
    pub fn builder() -> EngineConfigBuilder {
        EngineConfigBuilder::new()
    }

    pub fn db_path(&self) -> &DatabasePath {
        &self.db_path
    }

    pub fn socket_path(&self) -> &SocketPath {
        &self.socket_path
    }

    pub fn log_file_path(&self) -> &LogFilePath {
        &self.log_file_path
    }
}

#[derive(Default)]
pub struct EngineConfigBuilder {
    db_path: Option<DatabasePath>,
    socket_path: Option<SocketPath>,
    log_file_path: Option<LogFilePath>,
}

impl EngineConfigBuilder {
    fn new() -> Self {
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

    pub fn log_file_path(mut self, log_file_path: impl Into<LogFilePath>) -> Self {
        self.log_file_path = Some(log_file_path.into());
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

        let log_file_path = match self.log_file_path {
            Some(path) => path,
            None => LogFilePath::resolve(None)?,
        };

        Ok(EngineConfig {
            db_path,
            socket_path,
            log_file_path,
        })
    }
}

// ================ ENGINE LOCK UTIL ================
#[derive(Debug)]
struct EngineLock {
    file: File,
    path: PathBuf,
}

impl EngineLock {
    pub fn acquire(db_path: PathBuf, lock_path: PathBuf) -> EngineResult<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                EngineError::Io(format!("opening lock file {}: {e}", lock_path.display()))
            })?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                let pid = Self::read_lock_pid(&lock_path).unwrap_or_else(|| "unknown".to_string());
                tracing::warn!(
                    db_path = %db_path.display(),
                    lock_path = %lock_path.display(),
                    holder_pid = %pid,
                    "engine lock contended: another instance appears to be running"
                );
                return Err(EngineError::EngineAlreadyRunning { db_path, pid });
            }
            Err(TryLockError::Error(e)) => {
                return Err(EngineError::Io(format!(
                    "locking {}: {e}",
                    lock_path.display()
                )));
            }
        }

        // We own the lock: record our PID for diagnostics.
        Self::write_pid(&mut file)
            .map_err(|e| EngineError::Io(format!("writing pid to {}: {e}", lock_path.display())))?;

        tracing::debug!(lock_path = %lock_path.display(), pid = std::process::id(), "pid written to lock file");

        Ok(Self {
            file,
            path: lock_path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write_pid(file: &mut File) -> std::io::Result<()> {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(std::process::id().to_string().as_bytes())?;
        file.flush()
    }

    fn read_lock_pid(lock_path: &Path) -> Option<String> {
        let contents = std::fs::read_to_string(lock_path).ok()?;
        let pid = contents.trim();
        if pid.is_empty() {
            None
        } else {
            Some(pid.to_string())
        }
    }
}

impl Drop for EngineLock {
    fn drop(&mut self) {
        // Remove the file first, then let the kernel lock release when the handle closes: a starter
        // racing the removal still cannot acquire the flock until this handle is gone, and fresh
        // starts simply create a new inode.
        let _ = std::fs::remove_file(&self.path);
        let _ = self.file.unlock();

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine_stop",
            lock_path = %self.path.display(),
            "single-instance lock released"
        );
    }
}

// ================ ENGINE INITIALIZATION ================
struct Bootstrap<'c> {
    config: &'c EngineConfig,
}

impl<'c> Bootstrap<'c> {
    /// Start a boot sequence; logs the startup banner.
    pub fn new(config: &'c EngineConfig) -> Self {
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            db_path = %config.db_path.0.display(),
            socket_path = %config.socket_path.0.display(),
            // lock_path = %config.lock_path().display(),
            // maintenance_interval_secs = config.maintenance_interval().as_secs(),
            // heartbeat_interval_secs = config.heartbeat_interval().as_secs(),
            // durability = config.durability_label(),
            // reader_pool_size = READER_POOL_SIZE,
            "starting Valqeron Engine boot sequence"
        );
        Self { config }
    }

    /// Acquire the exclusive single-instance lock (`<db>.lock`).
    fn acquire_lock(self) -> EngineResult<Locked<'c>> {
        // Ensure parent exists
        if let Some(parent) = self.config.db_path.0.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
        }

        // Acquire the lock
        let lock_file = self.config.db_path.0.with_extension(".lock");
        let lock = EngineLock::acquire(self.config.db_path.0.to_path_buf(), lock_file)?;

        // Log the lock acquisition
        tracing::info!(
            target: "valqeron::audit",
            operation = "engine_start",
            lock_path = %lock.path().display(),
            "single-instance lock acquired"
        );

        Ok(Locked {
            config: self.config,
            lock,
        })
    }
}

/// Phase 1: single-instance lock held.
pub struct Locked<'c> {
    config: &'c EngineConfig,
    lock: EngineLock,
}

impl<'c> Locked<'c> {
    /// Create the 0700 socket directory and unlink any leftover socket file.
    /// Safe because we hold the exclusive lock: no live engine can be serving on it.
    pub fn prepare_socket(self) -> EngineResult<SocketReady<'c>> {
        Self::ensure_socket_dir(self.config.socket_path.0.to_path_buf())?;
        Self::remove_stale_socket(self.config.socket_path.0.as_path())?;

        tracing::info!(
            socket_dir = ?self.config.socket_path.0.parent(),
            "socket directory prepared and restricted to 0700"
        );

        Ok(SocketReady {
            config: self.config,
            lock: self.lock,
        })
    }

    /// Create the socket's parent directory and restrict it to `0700`.
    /// Directory permissions are the primary local access control for the engine socket.
    fn ensure_socket_dir(socket: PathBuf) -> EngineResult<()> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(parent) = socket.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| EngineError::Io(format!("restricting {}: {e}", parent.display())))?;
        }
        Ok(())
    }

    /// Unlink a leftover socket file. Safe because the caller holds the
    /// exclusive engine lock: no live engine can be serving on it.
    fn remove_stale_socket(path: &Path) -> EngineResult<()> {
        match std::fs::remove_file(path) {
            Ok(()) => {
                tracing::info!(socket = %path.display(), "removed stale socket file");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(EngineError::Io(format!(
                "removing stale socket {}: {e}",
                path.display()
            ))),
        }
    }
}

pub struct SocketReady<'c> {
    config: &'c EngineConfig,
    lock: EngineLock,
}

impl<'c> SocketReady<'c> {
    /// Open the database (migrations run here, before any reader exists) and
    /// wrap it in the lane-bounded storage facade.
    pub fn open_database(self) -> EngineResult<DatabaseOpen<'c>> {
        let db_config = DatabaseConfig {
            synchronous: Synchronous::Normal,
            ..DatabaseConfig::default()
        };

        let engine = SqliteStorageEngine::open(self.config.db_path.0.as_path(), db_config)?;

        tracing::info!(
            db_path = %self.config.db_path.0.display(),
            "database open; migrations applied"
        );

        // Lane sizing is derived from the engine's actual reader pool inside
        // AsyncStorage::new — permits ≡ connections by construction.
        let storage = AsyncStorage::new(engine);
        Ok(DatabaseOpen {
            config: self.config,
            lock: self.lock,
            storage,
        })
    }
}

/// Phase 3: database open, migrations applied, storage facade constructed.
pub struct DatabaseOpen<'c> {
    config: &'c EngineConfig,
    lock: EngineLock,
    storage: AsyncStorage,
}

impl<'c> DatabaseOpen<'c> {
    /// Bind the Unix listener synchronously and restrict the socket file.
    /// From this point connections queue in the backlog until serving starts;
    /// the socket's existence proves the database is open and migrated.
    pub fn bind_socket(self) -> EngineResult<Bound<'c>> {
        let socket_path = self.config.socket_path.0.as_path();
        let listener = StdUnixListener::bind(socket_path).map_err(|e| {
            EngineError::Io(format!("binding socket {}: {e}", socket_path.display()))
        })?;
        listener.set_nonblocking(true).map_err(|e| {
            EngineError::Io(format!(
                "setting {} nonblocking: {e}",
                socket_path.display()
            ))
        })?;
        Self::restrict_socket_file(socket_path)?;

        tracing::info!(
            socket_path = %socket_path.display(),
            "unix socket bound, set non-blocking, and restricted to 0600"
        );

        Ok(Bound {
            config: self.config,
            lock: self.lock,
            storage: self.storage,
            listener,
        })
    }

    /// Tighten the bound socket file itself (the 0700 parent directory is the primary control; this is
    /// a belt-and-braces).
    fn restrict_socket_file(path: &Path) -> EngineResult<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| EngineError::Io(format!("restricting {}: {e}", path.display())))
    }
}

/// Phase 4: listener bound and restricted — the readiness point of the boot sequence. The listener
/// is nonblocking, ready for tokio registration.
pub struct Bound<'c> {
    config: &'c EngineConfig,
    lock: EngineLock,
    storage: AsyncStorage,
    listener: StdUnixListener,
}

impl<'c> Bound<'c> {
    /// Build the multi-thread tokio runtime: named threads and a bounded
    /// blocking pool sized to the storage lanes.
    pub fn build_runtime(self) -> EngineResult<ValqeronEngine<'c>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("valqeron-worker")
            .build()
            .map_err(|e| EngineError::Io(format!("building tokio runtime: {e}")))?;

        tracing::info!(thread_name = "valqeron-worker", "tokio runtime initialized");

        Ok(ValqeronEngine {
            config: self.config,
            lock: self.lock,
            storage: self.storage,
            listener: self.listener,
            runtime,
        })
    }
}

// ================ ENGINE CONFIGURATION ================
pub struct ValqeronEngine<'c> {
    config: &'c EngineConfig,
    lock: EngineLock,
    storage: AsyncStorage,
    listener: StdUnixListener,
    runtime: tokio::runtime::Runtime,
}

impl ValqeronEngine<'_> {
    pub fn new(engine_config: &EngineConfig) -> ValqeronEngine<'_> {
        boot(engine_config).expect("boot failed")
    }

    pub fn start(self) -> EngineResult<()> {
        let Self {
            config,
            lock,
            storage,
            listener,
            runtime,
        } = self;

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine_start",
            db_path = %config.db_path.0.display(),
            socket_path = %config.socket_path.0.display(),
            lock_path = %lock.path().display(),
            "Valqeron Engine started and ready to serve"
        );

        Ok(())
    }
}

/// Drive the full boot sequence: acquire the single-instance lock, prepare
/// and bind the socket, open the database (running migrations), and build
/// the tokio runtime. This is the one public entry point into the phased
/// boot sequence above — the CLI's `start` command should call this.
pub fn boot(config: &EngineConfig) -> EngineResult<ValqeronEngine<'_>> {
    Bootstrap::new(config)
        .acquire_lock()?
        .prepare_socket()?
        .open_database()?
        .bind_socket()?
        .build_runtime()
}

async fn run_loop(
    storage: AsyncStorage,
    listener: StdUnixListener,
    config: &EngineConfig,
) -> EngineResult<()> {
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| EngineError::Io(format!("installing SIGTERM handler: {e}")))?;
    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|e| EngineError::Io(format!("installing SIGINT handler: {e}")))?;

    let started = Instant::now();

    // The listener was bound (nonblocking) during bootstrap; register it with
    // this runtime's reactor. Connections that queued in the backlog while
    // the runtime was being built are served as soon as tonic starts.
    let listener = UnixListener::from_std(listener).map_err(|e| {
        EngineError::Io(format!(
            "registering socket {} with the runtime: {e}",
            config.socket_path.0.display()
        ))
    })?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let issuer_service = RpcIssuerServiceServer::new(IssuerGrpc::new(storage.clone()));
    let admin_service = RpcAdminServiceServer::new(AdminGrpc::new(
        config.db_path.0.display().to_string(),
        started,
    ));

    let mut server = tokio::spawn(
        Server::builder()
            .add_service(issuer_service)
            .add_service(admin_service)
            .serve_with_incoming_shutdown(UnixListenerStream::new(listener), async move {
                let _ = shutdown_rx.await;
            }),
    );

    tracing::info!(
        target: "valqeron::audit",
        operation = "grpc_listen",
        socket = %config.socket_path.0.display(),
        "gRPC server listening"
    );

    // The deterministic readiness point: lock held, migrations applied,
    // socket bound, server task serving. Under a systemd Type=notify unit
    // this completes startup; everywhere else the notify call is a no-op.
    tracing::info!(
        target: "valqeron::audit",
        operation = "engine_ready",
        socket = %config.socket_path.0.display(),
        "engine ready"
    );
    crate::notify::notify_ready();

    // Background work runs as periodic jobs on their own tasks; the select
    // loop below stays a pure signal/server watcher.
    let mut jobs = JobSet::new();
    jobs.spawn(
        PeriodicJob {
            name: "db_maintenance",
            period: Duration::from_secs(1),
            jitter: true,
        },
        {
            let storage = storage.clone();
            move || {
                let storage = storage.clone();
                async move {
                    if let Err(e) = storage
                        .maintenance("db_maintenance", run_maintenance_job)
                        .await
                    {
                        tracing::warn!(
                            job = "db_maintenance",
                            error = %e,
                            "maintenance not executed"
                        );
                    }
                }
            }
        },
    );
    jobs.spawn(
        PeriodicJob {
            name: "heartbeat",
            period: Duration::from_secs(1),
            jitter: false,
        },
        move || async move {
            tracing::debug!(
                job = "heartbeat",
                uptime_secs = started.elapsed().as_secs(),
                "engine alive"
            );
        },
    );

    // Serve until a signal arrives or the server dies on its own; the
    // periodic jobs run on their own tasks in the meantime.
    let outcome: EngineResult<&'static str> = tokio::select! {
        _ = sigterm.recv() => Ok("SIGTERM"),
        _ = sigint.recv() => Ok("SIGINT"),
        joined = &mut server => Err(server_exit_error(joined)),
    };

    // Every continuation from here is a shutdown, clean or not.
    crate::notify::notify_stopping();

    let reason = match outcome {
        Ok(reason) => reason,
        Err(e) => {
            // The server died on its own; stop background work and bail.
            let _ = jobs.drain(Duration::from_secs(1)).await;
            storage.close();
            return Err(e);
        }
    };

    tracing::info!(
        signal = reason,
        "shutdown requested; draining in-flight RPCs and background work"
    );

    // Stop accepting connections and drain in-flight RPCs. A second signal
    // during the drain forces an immediate (non-zero) exit.
    let _ = shutdown_tx.send(());
    tokio::select! {
        _ = sigterm.recv() => { storage.close(); return Err(EngineError::ForcedShutdown); }
        _ = sigint.recv() => { storage.close(); return Err(EngineError::ForcedShutdown); }
        joined = &mut server => {
            if let Err(e) = server_join_outcome(joined) {
                tracing::warn!(error = %e, "gRPC server ended with an error during drain");
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            tracing::warn!(
                drain_timeout_secs = Duration::from_secs(10).as_secs(),
                "gRPC server did not drain within the deadline; aborting it"
            );
            server.abort();
        }
    }

    // Stop the periodic jobs (waiting for any body still in flight), then
    // reject new storage work and wait for in-flight closures to finish.
    if !jobs.drain(Duration::from_secs(10)).await {
        tracing::warn!(
            drain_timeout_secs = Duration::from_secs(10).as_secs(),
            "background jobs did not finish within the drain deadline"
        );
    }
    storage.close();
    if !storage.wait_idle(Duration::from_secs(10)).await {
        tracing::warn!(
            drain_timeout_secs = Duration::from_secs(10).as_secs(),
            "storage closures still in flight after the drain deadline"
        );
    }

    Ok(())
}

fn resolve_default_project_dir() -> Option<ProjectDirs> {
    ProjectDirs::from(
        DEFAULT_VALQERON_QUALIFIER,
        DEFAULT_VALQERON_ORGANIZATION,
        DEFAULT_VALQERON_APP,
    )
}

fn server_exit_error(
    joined: Result<Result<(), tonic::transport::Error>, tokio::task::JoinError>,
) -> EngineError {
    match joined {
        Ok(Ok(())) => EngineError::Io("gRPC server exited unexpectedly".to_string()),
        Ok(Err(e)) => EngineError::Io(format!("gRPC server failed: {e}")),
        Err(e) => EngineError::Io(format!("gRPC server task failed: {e}")),
    }
}

/// One maintenance run, executed through the storage facade — never on a
/// runtime thread. Failures are logged and retried on the next tick; they
/// must not take the daemon down.
fn run_maintenance_job(engine: &SqliteStorageEngine) {
    let started = Instant::now();
    match engine.run_maintenance() {
        Ok(stats) => tracing::info!(
            target: "valqeron::audit",
            operation = "db_maintenance",
            busy = stats.busy,
            wal_frames = stats.log_frames,
            checkpointed_frames = stats.checkpointed_frames,
            duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "maintenance completed"
        ),
        Err(e) => tracing::warn!(
            target: "valqeron::audit",
            operation = "db_maintenance",
            error = %e,
            "maintenance failed; retrying at the next interval"
        ),
    }
}

fn server_join_outcome(
    joined: Result<Result<(), tonic::transport::Error>, tokio::task::JoinError>,
) -> Result<(), String> {
    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
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

    #[test]
    fn test_log_file_path_resolve_errors_when_no_override_env_or_project_dir() {
        unsafe {
            std::env::remove_var(ENGINE_LOG_FILE_ENV);
        }

        let result = LogFilePath::resolve_with_project_log_file(None, None);

        assert!(matches!(
            result,
            Err(EngineError::InvalidDatabasePath {
                app: DEFAULT_VALQERON_APP,
                env_var: ENGINE_LOG_FILE_ENV,
            })
        ));
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("test.db");
        let lock = dir.path().join("test.db.lock");
        (dir, db, lock)
    }

    #[test]
    fn acquiring_writes_our_pid() {
        let (_dir, db, lock_path) = temp_paths();
        let lock = EngineLock::acquire(db.clone(), lock_path.clone()).unwrap();
        let pid = EngineLock::read_lock_pid(&lock_path).expect("pid recorded");
        assert_eq!(pid, std::process::id().to_string());
        drop(lock);
    }

    #[test]
    fn second_acquire_fails_with_already_running() {
        let (_dir, db, lock_path) = temp_paths();
        let _held = EngineLock::acquire(db.clone(), lock_path.clone()).unwrap();

        let err = EngineLock::acquire(db.clone(), lock_path).unwrap_err();
        match err {
            EngineError::EngineAlreadyRunning { db_path, pid } => {
                assert_eq!(db_path, db);
                assert_eq!(pid, std::process::id().to_string());
            }
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
    }

    #[test]
    fn dropping_removes_the_file_and_releases_the_lock() {
        let (_dir, db, lock_path) = temp_paths();
        let lock = EngineLock::acquire(db.clone(), lock_path.clone()).unwrap();
        drop(lock);
        assert!(!lock_path.exists(), "lock file removed on clean release");

        // Reacquire works immediately.
        let again = EngineLock::acquire(db.clone(), lock_path.clone());
        assert!(again.is_ok(), "lock must be reacquirable after release");
    }

    #[test]
    fn stale_lock_file_with_dead_content_does_not_block_acquisition() {
        let (_dir, db, lock_path) = temp_paths();
        // Simulate SIGKILL residue: a file exists but nobody holds the flock.
        std::fs::write(&lock_path, "99999999").unwrap();

        let lock = EngineLock::acquire(db.clone(), lock_path.clone()).unwrap();
        let pid = EngineLock::read_lock_pid(&lock_path).expect("pid overwritten");
        assert_eq!(pid, std::process::id().to_string());
        drop(lock);
    }
}

#[cfg(test)]
mod boot_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Builds a config pointing entirely inside `dir`, bypassing `resolve()`
    /// (and therefore env vars / OS project-dir lookups) so each test is
    /// fully isolated and safe to run in parallel.
    fn test_config(dir: &Path) -> EngineConfig {
        EngineConfig {
            db_path: DatabasePath(dir.join("test.db")),
            socket_path: SocketPath(dir.join("run").join("test.sock")),
            log_file_path: LogFilePath(dir.join("test.log")),
        }
    }

    #[test]
    fn boots_end_to_end_and_starts_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let engine = boot(&config).expect("engine should boot");
        assert!(
            config.socket_path.0.exists(),
            "socket file should exist once bound"
        );
        engine.start().expect("engine should start");
    }

    #[test]
    fn socket_dir_and_file_have_restricted_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let engine = boot(&config).unwrap();

        let dir_mode = std::fs::metadata(config.socket_path.0.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700, "socket parent dir should be 0700");

        let file_mode = std::fs::metadata(&config.socket_path.0)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600, "socket file should be 0600");

        engine.start().unwrap();
    }

    #[test]
    fn second_boot_against_same_db_fails_while_first_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let _first = boot(&config).expect("first boot should succeed");
        let second = boot(&config);

        assert!(matches!(
            second,
            Err(EngineError::EngineAlreadyRunning { .. })
        ));
    }

    #[test]
    fn boot_succeeds_again_after_previous_engine_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let first = boot(&config).expect("first boot should succeed");
        drop(first);

        let second = boot(&config);
        assert!(
            second.is_ok(),
            "boot should succeed again once the lock is released"
        );
    }

    #[test]
    fn stale_socket_file_does_not_block_a_fresh_boot() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        // Simulate a leftover socket file from a crashed engine (no lock held).
        std::fs::create_dir_all(config.socket_path.0.parent().unwrap()).unwrap();
        std::fs::write(&config.socket_path.0, b"not a real socket").unwrap();

        let engine = boot(&config);
        assert!(
            engine.is_ok(),
            "a stale socket file must not block a fresh boot"
        );
    }

    #[test]
    fn missing_parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("does").join("not").join("exist");
        let config = EngineConfig {
            db_path: DatabasePath(nested.join("test.db")),
            socket_path: SocketPath(nested.join("run").join("test.sock")),
            log_file_path: LogFilePath(nested.join("test.log")),
        };

        let engine = boot(&config);
        assert!(
            engine.is_ok(),
            "boot should create missing db and socket parent directories"
        );
    }
}

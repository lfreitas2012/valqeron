// //! Phased engine bootstrap with type-enforced ordering.
// //!
// //! Startup is a linear chain in which every phase consumes the previous one, so the strict resource
// //! order — lock → socket dir → stale-socket removal → database (migrations) → bind → runtime — cannot
// //! be rearranged without failing to compile:
// //!
// //! ```text
// //! Bootstrap::new(config)
// //!     .acquire_lock()?      exclusive flock on <db>.lock
// //!     .prepare_socket()?    0700 socket dir; unlink stale socket (safe: lock held)
// //!     .open_database()?     open SQLite, run migrations, wrap in AsyncStorage
// //!     .bind_socket()?       bind the std listener, 0600 — socket exists ⇒ DB migrated
// //!     .build_runtime()?     multi_thread tokio, named threads, capped blocking pool
// //!     .run()                serve; then ordered teardown (below)
// //! ```
// //!
// //! Binding happens synchronously *before* the runtime starts: readiness is a deterministic point in
// //! the boot sequence, and a client connecting in the window before serving queues in the listener
// //! backlog instead of racing the socket file into existence. Because the bind follows `open_database`,
// //! the historical invariant "socket exists ⇒ database is open and migrated" is preserved.
// //!
// //! Teardown runs in [`BootedEngine::run`] in the exact reverse-dependency order the checkpoint
// //! requires: drain the runtime → unlink the socket → reclaim and drop the engine
// //! (`PRAGMA optimize` + `wal_checkpoint(TRUNCATE)`) → release the lock last, after the checkpoint
// //! proves the database is quiesced.
//
// use std::os::unix::net::UnixListener as StdUnixListener;
// use std::path::Path;
//
// use crate::error::{EngineError, EngineResult};
// use crate::grpc::{AdminGrpc, IssuerGrpc};
// use crate::jobs::{JobSet, PeriodicJob};
// use crate::lockfile::EngineLock;
// use crate::runtime::EngineConfig;
// use crate::runtime::config::READER_POOL_SIZE;
// use crate::storage::AsyncStorage;
// use std::time::{Duration, Instant};
// use tokio::net::UnixListener;
// use tokio::signal::unix::{SignalKind, signal};
// use tokio_stream::wrappers::UnixListenerStream;
// use tonic::transport::Server;
// use valqeron_infrastructure::{DatabaseConfig, SqliteStorageEngine};
// use valqeron_proto::v1::rpc_admin_service_server::RpcAdminServiceServer;
// use valqeron_proto::v1::rpc_issuer_service_server::RpcIssuerServiceServer;
//
// /// Bound on draining the blocking pool when the runtime shuts down. A stuck job must not hold the
// /// process open forever — the service manager's SIGKILL (after `TimeoutStopSec` / launchd's exit
// /// timeout) is the final backstop.
// const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);
//
// /// Blocking-pool cap: every storage lane slot may hold a blocking thread
// /// (readers + the single writer), plus a small margin for tokio's own
// /// internal blocking work. The lanes bound admission; this bounds the pool
// /// itself (tokio's default is 512).
// const MAX_BLOCKING_THREADS: usize = READER_POOL_SIZE
//     .saturating_add(1) // the single writer
//     .saturating_add(2); // margin
//
// /// How long the graceful shutdown waits for in-flight RPCs / jobs.
// const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
//
// /// Phase 0: nothing acquired yet.
// pub struct Bootstrap<'c> {
//     config: &'c EngineConfig,
// }
//
// /// Phase 1: single-instance lock held.
// pub struct Locked<'c> {
//     config: &'c EngineConfig,
//     lock: EngineLock,
// }
//
// /// Phase 2: socket directory prepared, stale socket removed (under the lock).
// pub struct SocketReady<'c> {
//     config: &'c EngineConfig,
//     lock: EngineLock,
// }
//
// /// Phase 3: database open, migrations applied, storage facade constructed.
// pub struct DatabaseOpen<'c> {
//     config: &'c EngineConfig,
//     lock: EngineLock,
//     storage: AsyncStorage,
// }
//
// /// Phase 4: listener bound and restricted — the readiness point of the boot sequence. The listener
// /// is nonblocking, ready for tokio registration.
// pub struct Bound<'c> {
//     config: &'c EngineConfig,
//     lock: EngineLock,
//     storage: AsyncStorage,
//     listener: StdUnixListener,
// }
//
// /// Phase 5: runtime built; ready to serve.
// pub struct BootedEngine<'c> {
//     config: &'c EngineConfig,
//     lock: EngineLock,
//     storage: AsyncStorage,
//     listener: StdUnixListener,
//     runtime: tokio::runtime::Runtime,
// }
//
// impl<'c> Bootstrap<'c> {
//     /// Start a boot sequence; logs the startup banner.
//     pub fn new(config: &'c EngineConfig) -> Self {
//         tracing::info!(
//             version = env!("CARGO_PKG_VERSION"),
//             db_path = %config.db_path().display(),
//             socket_path = %config.socket_path().display(),
//             lock_path = %config.lock_path().display(),
//             maintenance_interval_secs = config.maintenance_interval().as_secs(),
//             heartbeat_interval_secs = config.heartbeat_interval().as_secs(),
//             durability = config.durability_label(),
//             reader_pool_size = READER_POOL_SIZE,
//             "Starting Valqeron Engine..."
//         );
//         Self { config }
//     }
//
//     /// Acquire the exclusive single-instance lock (`<db>.lock`).
//     pub fn acquire_lock(self) -> EngineResult<Locked<'c>> {
//         self.config.ensure_db_parent()?;
//         let lock = EngineLock::acquire(self.config.db_path(), self.config.lock_path())?;
//         tracing::info!(
//             target: "valqeron::audit",
//             operation = "engine_start",
//             lock_path = %lock.path().display(),
//             "single-instance lock acquired"
//         );
//         Ok(Locked {
//             config: self.config,
//             lock,
//         })
//     }
// }
//
// impl<'c> Locked<'c> {
//     /// Create the 0700 socket directory and unlink any leftover socket file.
//     /// Safe because we hold the exclusive lock: no live engine can be serving
//     /// on it.
//     pub fn prepare_socket(self) -> EngineResult<SocketReady<'c>> {
//         self.config.ensure_socket_dir()?;
//         remove_stale_socket(self.config.socket_path())?;
//         Ok(SocketReady {
//             config: self.config,
//             lock: self.lock,
//         })
//     }
// }
//
// impl<'c> SocketReady<'c> {
//     /// Open the database (migrations run here, before any reader exists) and
//     /// wrap it in the lane-bounded storage facade.
//     pub fn open_database(self) -> EngineResult<DatabaseOpen<'c>> {
//         let db_config = DatabaseConfig {
//             reader_pool_size: READER_POOL_SIZE,
//             synchronous: self.config.synchronous(),
//             ..DatabaseConfig::default()
//         };
//         let engine = SqliteStorageEngine::open(self.config.db_path(), db_config)?;
//         tracing::info!("database open; migrations applied");
//         // Lane sizing is derived from the engine's actual reader pool inside
//         // AsyncStorage::new — permits ≡ connections by construction.
//         let storage = AsyncStorage::new(engine);
//         Ok(DatabaseOpen {
//             config: self.config,
//             lock: self.lock,
//             storage,
//         })
//     }
// }
//
// impl<'c> DatabaseOpen<'c> {
//     /// Bind the Unix listener synchronously and restrict the socket file.
//     /// From this point connections queue in the backlog until serving starts;
//     /// the socket's existence proves the database is open and migrated.
//     pub fn bind_socket(self) -> EngineResult<Bound<'c>> {
//         let socket_path = self.config.socket_path();
//         let listener = StdUnixListener::bind(socket_path).map_err(|e| {
//             EngineError::Io(format!("binding socket {}: {e}", socket_path.display()))
//         })?;
//         listener.set_nonblocking(true).map_err(|e| {
//             EngineError::Io(format!(
//                 "setting {} nonblocking: {e}",
//                 socket_path.display()
//             ))
//         })?;
//         restrict_socket_file(socket_path)?;
//         Ok(Bound {
//             config: self.config,
//             lock: self.lock,
//             storage: self.storage,
//             listener,
//         })
//     }
// }
//
// impl<'c> Bound<'c> {
//     /// Build the multi-thread tokio runtime: named threads and a bounded
//     /// blocking pool sized to the storage lanes.
//     pub fn build_runtime(self) -> EngineResult<BootedEngine<'c>> {
//         let runtime = tokio::runtime::Builder::new_multi_thread()
//             .enable_all()
//             .thread_name("valqeron-worker")
//             .max_blocking_threads(MAX_BLOCKING_THREADS)
//             .build()
//             .map_err(|e| EngineError::Io(format!("building tokio runtime: {e}")))?;
//         Ok(BootedEngine {
//             config: self.config,
//             lock: self.lock,
//             storage: self.storage,
//             listener: self.listener,
//             runtime,
//         })
//     }
// }
//
// impl BootedEngine<'_> {
//     /// Serve until shutdown, then tear down in checkpoint-safe order.
//     pub fn run(self) -> EngineResult<()> {
//         let Self {
//             config,
//             lock,
//             storage,
//             listener,
//             runtime,
//         } = self;
//
//         let loop_result = runtime.block_on(run_loop(storage.clone(), listener, config));
//
//         // Wait (bounded) for any still-running blocking task before the final
//         // checkpoint; queued-but-unstarted tasks are dropped.
//         runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
//
//         let _ = std::fs::remove_file(config.socket_path());
//
//         match storage.into_engine() {
//             Ok(engine) => {
//                 // Drop = PRAGMA optimize + wal_checkpoint(TRUNCATE).
//                 drop(engine);
//                 tracing::info!("final WAL checkpoint complete");
//             }
//             Err(_still_shared) => {
//                 tracing::warn!(
//                     "a storage task still holds the engine; skipping the final checkpoint"
//                 );
//             }
//         }
//
//         drop(lock);
//         tracing::info!(
//             target: "valqeron::audit",
//             operation = "engine_stop",
//             "engine stopped cleanly"
//         );
//         loop_result
//     }
// }
//
// async fn run_loop(
//     storage: AsyncStorage,
//     listener: StdUnixListener,
//     config: &EngineConfig,
// ) -> EngineResult<()> {
//     let mut sigterm = signal(SignalKind::terminate())
//         .map_err(|e| EngineError::Io(format!("installing SIGTERM handler: {e}")))?;
//     let mut sigint = signal(SignalKind::interrupt())
//         .map_err(|e| EngineError::Io(format!("installing SIGINT handler: {e}")))?;
//
//     let started = Instant::now();
//
//     // The listener was bound (nonblocking) during bootstrap; register it with
//     // this runtime's reactor. Connections that queued in the backlog while
//     // the runtime was being built are served as soon as tonic starts.
//     let listener = UnixListener::from_std(listener).map_err(|e| {
//         EngineError::Io(format!(
//             "registering socket {} with the runtime: {e}",
//             config.socket_path().display()
//         ))
//     })?;
//
//     let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
//     let issuer_service = RpcIssuerServiceServer::new(IssuerGrpc::new(storage.clone()));
//     let admin_service = RpcAdminServiceServer::new(AdminGrpc::new(
//         config.db_path().display().to_string(),
//         started,
//     ));
//
//     let mut server = tokio::spawn(
//         Server::builder()
//             .add_service(issuer_service)
//             .add_service(admin_service)
//             .serve_with_incoming_shutdown(UnixListenerStream::new(listener), async move {
//                 let _ = shutdown_rx.await;
//             }),
//     );
//
//     tracing::info!(
//         target: "valqeron::audit",
//         operation = "grpc_listen",
//         socket = %config.socket_path().display(),
//         "gRPC server listening"
//     );
//
//     // The deterministic readiness point: lock held, migrations applied,
//     // socket bound, server task serving. Under a systemd Type=notify unit
//     // this completes startup; everywhere else the notify call is a no-op.
//     tracing::info!(
//         target: "valqeron::audit",
//         operation = "engine_ready",
//         socket = %config.socket_path().display(),
//         "engine ready"
//     );
//     crate::notify::notify_ready();
//
//     // Background work runs as periodic jobs on their own tasks; the select
//     // loop below stays a pure signal/server watcher.
//     let mut jobs = JobSet::new();
//     jobs.spawn(
//         PeriodicJob {
//             name: "db_maintenance",
//             period: config.maintenance_interval(),
//             jitter: true,
//         },
//         {
//             let storage = storage.clone();
//             move || {
//                 let storage = storage.clone();
//                 async move {
//                     if let Err(e) = storage
//                         .maintenance("db_maintenance", run_maintenance_job)
//                         .await
//                     {
//                         tracing::warn!(
//                             job = "db_maintenance",
//                             error = %e,
//                             "maintenance not executed"
//                         );
//                     }
//                 }
//             }
//         },
//     );
//     jobs.spawn(
//         PeriodicJob {
//             name: "heartbeat",
//             period: config.heartbeat_interval(),
//             jitter: false,
//         },
//         move || async move {
//             tracing::debug!(
//                 job = "heartbeat",
//                 uptime_secs = started.elapsed().as_secs(),
//                 "engine alive"
//             );
//         },
//     );
//
//     // Serve until a signal arrives or the server dies on its own; the
//     // periodic jobs run on their own tasks in the meantime.
//     let outcome: EngineResult<&'static str> = tokio::select! {
//         _ = sigterm.recv() => Ok("SIGTERM"),
//         _ = sigint.recv() => Ok("SIGINT"),
//         joined = &mut server => Err(server_exit_error(joined)),
//     };
//
//     // Every continuation from here is a shutdown, clean or not.
//     crate::notify::notify_stopping();
//
//     let reason = match outcome {
//         Ok(reason) => reason,
//         Err(e) => {
//             // The server died on its own; stop background work and bail.
//             let _ = jobs.drain(DRAIN_TIMEOUT).await;
//             storage.close();
//             return Err(e);
//         }
//     };
//
//     tracing::info!(
//         signal = reason,
//         "shutdown requested; draining in-flight RPCs and background work"
//     );
//
//     // Stop accepting connections and drain in-flight RPCs. A second signal
//     // during the drain forces an immediate (non-zero) exit.
//     let _ = shutdown_tx.send(());
//     tokio::select! {
//         _ = sigterm.recv() => { storage.close(); return Err(EngineError::ForcedShutdown); }
//         _ = sigint.recv() => { storage.close(); return Err(EngineError::ForcedShutdown); }
//         joined = &mut server => {
//             if let Err(e) = server_join_outcome(joined) {
//                 tracing::warn!(error = %e, "gRPC server ended with an error during drain");
//             }
//         }
//         _ = tokio::time::sleep(DRAIN_TIMEOUT) => {
//             tracing::warn!(
//                 drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
//                 "gRPC server did not drain within the deadline; aborting it"
//             );
//             server.abort();
//         }
//     }
//
//     // Stop the periodic jobs (waiting for any body still in flight), then
//     // reject new storage work and wait for in-flight closures to finish.
//     if !jobs.drain(DRAIN_TIMEOUT).await {
//         tracing::warn!(
//             drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
//             "background jobs did not finish within the drain deadline"
//         );
//     }
//     storage.close();
//     if !storage.wait_idle(DRAIN_TIMEOUT).await {
//         tracing::warn!(
//             drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
//             "storage closures still in flight after the drain deadline"
//         );
//     }
//
//     Ok(())
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use clap::Parser;
//
//     fn config_for(dir: &Path) -> EngineConfig {
//         let db = dir.join("boot.db");
//         let socket = dir.join("boot.sock");
//         let cli = crate::cli::Cli::try_parse_from([
//             "valqeron-engine",
//             "--db-path",
//             db.to_str().unwrap(),
//             "--socket",
//             socket.to_str().unwrap(),
//             "run",
//             "--no-log-file",
//         ])
//         .unwrap();
//         let crate::cli::Command::Run(run) = &cli.command else {
//             panic!("expected run subcommand");
//         };
//         EngineConfig::resolve(&cli, run).unwrap()
//     }
//
//     #[test]
//     fn bootstrap_chain_acquires_resources_in_order() {
//         let dir = tempfile::tempdir().unwrap();
//         let config = config_for(dir.path());
//
//         let locked = Bootstrap::new(&config).acquire_lock().unwrap();
//         assert!(config.lock_path().exists(), "lock file after acquire_lock");
//         assert!(!config.db_path().exists(), "no db before open_database");
//
//         let bound = locked
//             .prepare_socket()
//             .unwrap()
//             .open_database()
//             .unwrap()
//             .bind_socket()
//             .unwrap();
//         assert!(config.db_path().exists(), "db exists after open_database");
//         assert!(
//             config.socket_path().exists(),
//             "socket exists after bind_socket"
//         );
//
//         use std::os::unix::fs::PermissionsExt;
//         let mode = std::fs::metadata(config.socket_path())
//             .unwrap()
//             .permissions()
//             .mode();
//         assert_eq!(mode & 0o777, 0o600, "socket file restricted to 0600");
//
//         let booted = bound.build_runtime().unwrap();
//         drop(booted);
//         assert!(
//             !config.lock_path().exists(),
//             "dropping the boot chain releases the lock"
//         );
//     }
//
//     #[test]
//     fn stale_socket_removal_ignores_missing_files() {
//         let dir = tempfile::tempdir().unwrap();
//         let path = dir.path().join("missing.sock");
//         assert!(remove_stale_socket(&path).is_ok());
//
//         std::fs::write(&path, b"stale").unwrap();
//         assert!(remove_stale_socket(&path).is_ok());
//         assert!(!path.exists());
//     }
//
//     #[test]
//     fn second_bootstrap_fails_fast_while_lock_is_held() {
//         let dir = tempfile::tempdir().unwrap();
//         let config = config_for(dir.path());
//
//         let _held = Bootstrap::new(&config).acquire_lock().unwrap();
//         let second = Bootstrap::new(&config).acquire_lock();
//         assert!(matches!(second, Err(EngineError::AlreadyRunning { .. })));
//     }
// }

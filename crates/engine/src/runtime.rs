use std::path::Path;
use std::time::{Duration, Instant};

use crate::config::{EngineConfig, MAX_IN_FLIGHT_STORAGE, READER_POOL_SIZE};
use crate::error::{EngineError, EngineResult};
use crate::grpc::{AdminGrpc, IssuerGrpc};
use crate::lockfile::EngineLock;
use crate::storage::AsyncStorage;
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::MissedTickBehavior;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use valqeron_infrastructure::{DatabaseConfig, SqliteStorageEngine};
use valqeron_proto::v1::rpc_admin_service_server::RpcAdminServiceServer;
use valqeron_proto::v1::rpc_issuer_service_server::RpcIssuerServiceServer;

/// How long the graceful shutdown waits for in-flight RPCs / jobs.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound on draining the blocking pool when the runtime shuts down. A stuck
/// job must not hold the process open forever — the service manager's
/// SIGKILL (after `TimeoutStopSec` / launchd's exit timeout) is the final
/// backstop.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);

/// Foreground engine entry point: lock → open (migrations) → serve gRPC +
/// scheduled jobs → ordered shutdown.
///
/// Shutdown order matters: the server stops accepting and drains in-flight
/// RPCs first, then background work stops, and only then does the storage
/// engine drop — `Drop` runs `PRAGMA optimize` plus a
/// `wal_checkpoint(TRUNCATE)` that must not race in-flight writes. The lock
/// releases last, after the checkpoint proves the database is quiesced.
pub fn run(config: &EngineConfig) -> EngineResult<()> {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        db_path = %config.db_path().display(),
        socket_path = %config.socket_path().display(),
        lock_path = %config.lock_path().display(),
        maintenance_interval_secs = config.maintenance_interval().as_secs(),
        heartbeat_interval_secs = config.heartbeat_interval().as_secs(),
        durability = config.durability_label(),
        reader_pool_size = READER_POOL_SIZE,
        "valqeron-engine starting"
    );

    config.ensure_db_parent()?;
    let lock = EngineLock::acquire(config.db_path(), config.lock_path())?;
    tracing::info!(
        target: "valqeron::audit",
        operation = "engine_start",
        lock_path = %lock.path().display(),
        "single-instance lock acquired"
    );

    // We hold the exclusive lock, so any existing socket file is residue
    // from a previous instance (crash) — remove it before binding.
    config.ensure_socket_dir()?;
    remove_stale_socket(config.socket_path())?;

    let db_config = DatabaseConfig {
        reader_pool_size: READER_POOL_SIZE,
        synchronous: config.synchronous(),
        ..DatabaseConfig::default()
    };
    let engine = SqliteStorageEngine::open(config.db_path(), db_config)?;
    tracing::info!("database open; migrations applied");
    let storage = AsyncStorage::new(engine, MAX_IN_FLIGHT_STORAGE);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| EngineError::Io(format!("building tokio runtime: {e}")))?;

    let loop_result = runtime.block_on(run_loop(storage.clone(), config));

    // Wait (bounded) for any still-running blocking task before the final
    // checkpoint; queued-but-unstarted tasks are dropped.
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);

    let _ = std::fs::remove_file(config.socket_path());

    match storage.into_engine() {
        Ok(engine) => {
            // Drop = PRAGMA optimize + wal_checkpoint(TRUNCATE).
            drop(engine);
            tracing::info!("final WAL checkpoint complete");
        }
        Err(_still_shared) => {
            tracing::warn!("a storage task still holds the engine; skipping the final checkpoint");
        }
    }

    drop(lock);
    tracing::info!(
        target: "valqeron::audit",
        operation = "engine_stop",
        "engine stopped cleanly"
    );
    loop_result
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

async fn run_loop(storage: AsyncStorage, config: &EngineConfig) -> EngineResult<()> {
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| EngineError::Io(format!("installing SIGTERM handler: {e}")))?;
    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|e| EngineError::Io(format!("installing SIGINT handler: {e}")))?;

    let started = Instant::now();

    let listener = UnixListener::bind(config.socket_path()).map_err(|e| {
        EngineError::Io(format!(
            "binding socket {}: {e}",
            config.socket_path().display()
        ))
    })?;
    restrict_socket_file(config.socket_path())?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let issuer_service = RpcIssuerServiceServer::new(IssuerGrpc::new(storage.clone()));
    let admin_service = RpcAdminServiceServer::new(AdminGrpc::new(
        config.db_path().display().to_string(),
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
        socket = %config.socket_path().display(),
        "gRPC server listening"
    );

    // First ticks are one full period out (the startup banner already covers
    // "the engine is alive"); the maintenance period carries jitter so
    // periodic jobs do not synchronize with another periodic load.
    let maintenance_period = jittered(config.maintenance_interval());
    let mut maintenance = tokio::time::interval_at(
        tokio::time::Instant::now()
            .checked_add(maintenance_period)
            .unwrap_or_else(tokio::time::Instant::now),
        maintenance_period,
    );
    maintenance.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let heartbeat_period = config.heartbeat_interval();
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now()
            .checked_add(heartbeat_period)
            .unwrap_or_else(tokio::time::Instant::now),
        heartbeat_period,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut in_flight: Option<tokio::task::JoinHandle<()>> = None;

    let outcome: EngineResult<&'static str> = loop {
        tokio::select! {
            _ = sigterm.recv() => break Ok("SIGTERM"),
            _ = sigint.recv() => break Ok("SIGINT"),
            joined = &mut server => {
                break Err(server_exit_error(joined));
            }
            _ = maintenance.tick() => {
                let busy = in_flight.as_ref().is_some_and(|h| !h.is_finished());
                if busy {
                    tracing::warn!(
                        job = "db_maintenance",
                        "previous run still in flight; skipping this tick"
                    );
                } else {
                    let storage = storage.clone();
                    in_flight = Some(tokio::spawn(async move {
                        if let Err(e) = storage.call("db_maintenance", run_maintenance_job).await {
                            tracing::warn!(
                                job = "db_maintenance",
                                error = %e,
                                "maintenance not executed"
                            );
                        }
                    }));
                }
            }
            _ = heartbeat.tick() => {
                tracing::info!(
                    job = "heartbeat",
                    uptime_secs = started.elapsed().as_secs(),
                    "engine alive"
                );
            }
        }
    };

    let reason = match outcome {
        Ok(reason) => reason,
        Err(e) => {
            // The server died on its own; stop background work and bail.
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
        _ = tokio::time::sleep(DRAIN_TIMEOUT) => {
            tracing::warn!(
                drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
                "gRPC server did not drain within the deadline; aborting it"
            );
            server.abort();
        }
    }

    // No new storage work; wait for in-flight closures (RPC remnants and
    // the maintenance job) to finish.
    storage.close();
    if let Some(handle) = in_flight.take()
        && !handle.is_finished()
        && tokio::time::timeout(DRAIN_TIMEOUT, handle).await.is_err()
    {
        tracing::warn!(
            drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
            "maintenance task did not finish within the drain deadline"
        );
    }
    if !storage.wait_idle(DRAIN_TIMEOUT).await {
        tracing::warn!(
            drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
            "storage closures still in flight after the drain deadline"
        );
    }

    Ok(())
}

/// Tighten the bound socket file itself (the 0700 parent directory is the primary control; this is
/// a belt-and-braces).
fn restrict_socket_file(path: &Path) -> EngineResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| EngineError::Io(format!("restricting {}: {e}", path.display())))
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

fn server_join_outcome(
    joined: Result<Result<(), tonic::transport::Error>, tokio::task::JoinError>,
) -> Result<(), String> {
    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
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

/// Scale a period into 90%..=110% using clock sub-second noise — enough to desynchronize periodic jobs without pulling in an RNG dependency.
fn jittered(base: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let percent = u64::from(90u32.saturating_add(nanos.checked_rem(21).unwrap_or(0)));
    let millis = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let scaled = millis
        .saturating_mul(percent)
        .checked_div(100)
        .unwrap_or(millis);
    if scaled == 0 {
        base
    } else {
        Duration::from_millis(scaled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_within_ten_percent() {
        let base = Duration::from_secs(3600);
        for _ in 0..50 {
            let j = jittered(base);
            assert!(
                j >= Duration::from_secs(3240) && j <= Duration::from_secs(3960),
                "jittered value out of ±10% envelope: {j:?}"
            );
        }
    }

    #[test]
    fn jitter_never_zeroes_a_tiny_period() {
        assert!(jittered(Duration::from_millis(1)) > Duration::ZERO);
    }

    #[test]
    fn stale_socket_removal_ignores_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.sock");
        assert!(remove_stale_socket(&path).is_ok());

        std::fs::write(&path, b"stale").unwrap();
        assert!(remove_stale_socket(&path).is_ok());
        assert!(!path.exists());
    }
}

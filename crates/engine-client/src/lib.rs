//! Client for the Valqeron engine's gRPC API.
//!
//! Every gRPC service the engine exposes gets its own thin service type —
//! [`AdminService`], [`IssuerService`], and more as the engine grows —
//! reached through an accessor on the client:
//!
//! ```ignore
//! // Desktop app, already on an async runtime:
//! let client = AsyncClient::connect(ClientOptions::default()).await?;
//! let issuer = client.issuers().get(&id).await?;
//! client.admin().health().await?;
//!
//! // CLI, plain synchronous main:
//! let client = Client::connect(ClientOptions::default())?;
//! let issuer = client.issuers().get(&id)?;
//! client.admin().health()?;
//! ```
//!
//! [`AsyncClient`] and [`Client`] are two facades over the same async core
//! (`inner::Inner`): `AsyncClient` for consumers that already run their own
//! async runtime (the desktop app), `Client` for consumers with none (the
//! CLI) — it owns a runtime internally and blocks.

mod admin;
mod blocking;
mod inner;
mod issuer;

pub use admin::AdminService;
pub use blocking::{BlockingAdmin, BlockingIssuers};
pub use issuer::{DeleteIssuerRequest, IssuerService, PatchIssuerRequest, RegisterIssuerRequest};

use crate::inner::Inner;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub socket: Option<PathBuf>,
    pub connect_timeout: Option<Duration>,
    pub request_timeout: Option<Duration>,
}

impl ClientOptions {
    /// Connect to a specific socket instead of the resolved default.
    /// Useful for pointing at a non-default engine instance, and for tests
    /// that need an isolated socket.
    pub fn with_socket(mut self, socket: impl Into<PathBuf>) -> Self {
        self.socket = Some(socket.into());
        self
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }
}

#[derive(Debug, Clone)]
pub struct EngineInfo {
    pub engine_version: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone)]
pub struct EngineStatus {
    pub engine_version: String,
    pub protocol_version: u32,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackgroundTaskDetail {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(
        "no engine is running on {socket}; install and start the engine service \
         (during development: `just engine-install`)"
    )]
    NotRunning { socket: PathBuf },

    #[error("engine unreachable on {socket}: {message}")]
    Unreachable { socket: PathBuf, message: String },

    #[error(
        "engine {engine_version} speaks protocol v{engine_protocol}, but this client \
         (v{client_version}) requires protocol v{client_protocol}; upgrade the older side"
    )]
    VersionMismatch {
        client_protocol: u32,
        engine_protocol: u32,
        client_version: String,
        engine_version: String,
    },

    /// The engine rejected the call: a gRPC code plus the engine error's own
    /// human-readable message (the whole error contract since protocol v2).
    #[error("rpc failed ({code}): {message}")]
    Rpc { code: String, message: String },

    #[error("client configuration error: {0}")]
    Config(String),

    #[error("invalid response from engine: {0}")]
    InvalidResponse(String),

    #[error("failed to start the client runtime: {0}")]
    Runtime(String),
}

impl ClientError {
    pub fn is_not_running(&self) -> bool {
        matches!(self, ClientError::NotRunning { .. })
    }
}

/// Shared by every service module so each RPC's `tonic::Status` maps to the
/// same `ClientError` shape without every service reimplementing it.
pub(crate) fn map_status(socket: &Path, status: tonic::Status) -> ClientError {
    match status.code() {
        tonic::Code::Unavailable => ClientError::Unreachable {
            socket: socket.to_path_buf(),
            message: status.message().to_string(),
        },
        code => ClientError::Rpc {
            code: format!("{code:?}"),
            message: status.message().to_string(),
        },
    }
}

/// Async facade over a running engine. Use this from a consumer that already
/// drives its own async runtime (e.g. the desktop app). Cheap to share:
/// wrap in `Arc` for concurrent use across tasks.
pub struct AsyncClient(Inner);

impl AsyncClient {
    pub async fn connect(options: ClientOptions) -> Result<Self, ClientError> {
        Ok(Self(Inner::connect(options).await?))
    }

    pub fn socket(&self) -> &Path {
        self.0.socket()
    }

    pub fn engine_info(&self) -> &EngineInfo {
        self.0.engine_info()
    }

    pub fn admin(&self) -> AdminService {
        self.0.admin()
    }

    pub fn issuers(&self) -> IssuerService {
        self.0.issuers()
    }

    // pub fn securities(&self) -> SecurityService { self.0.securities() }
}

/// Blocking facade over a running engine. Owns a single-threaded `tokio`
/// runtime internally, so consumers with no async context of their own —
/// namely the CLI — get plain synchronous methods with no `.await`
/// anywhere. Don't call this from inside another async runtime's worker
/// thread; use [`AsyncClient`] there instead.
pub struct Client {
    runtime: tokio::runtime::Runtime,
    inner: Inner,
}

impl Client {
    /// Resolve the socket, connect with a bounded timeout, and run the version handshake.
    pub fn connect(options: ClientOptions) -> Result<Self, ClientError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ClientError::Runtime(e.to_string()))?;
        let inner = runtime.block_on(Inner::connect(options))?;
        Ok(Self { runtime, inner })
    }

    pub fn socket(&self) -> &Path {
        self.inner.socket()
    }

    pub fn engine_info(&self) -> &EngineInfo {
        self.inner.engine_info()
    }

    pub fn admin(&self) -> BlockingAdmin<'_> {
        BlockingAdmin::new(&self.runtime, self.inner.admin())
    }

    pub fn issuers(&self) -> BlockingIssuers<'_> {
        BlockingIssuers::new(&self.runtime, self.inner.issuers())
    }

    // pub fn securities(&self) -> BlockingSecurities<'_> {
    //     BlockingSecurities::new(&self.runtime, self.inner.securities())
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_socket_is_a_distinct_not_running_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("absent.sock");
        let result = Client::connect(ClientOptions {
            socket: Some(socket.clone()),
            ..ClientOptions::default()
        });
        let Err(err) = result else {
            panic!("connect must fail on a missing socket");
        };
        match err {
            ClientError::NotRunning { socket: reported } => assert_eq!(reported, socket),
            other => panic!("expected NotRunning, got {other:?}"),
        }
    }

    #[test]
    fn stale_socket_file_maps_to_not_running() {
        // A socket file nobody listens on: connect is refused.
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("stale.sock");
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        // The listener is closed but the file remains.
        assert!(socket.exists());

        let result = Client::connect(ClientOptions {
            socket: Some(socket),
            connect_timeout: Some(Duration::from_millis(500)),
            ..ClientOptions::default()
        });
        match result {
            Err(e) => assert!(
                e.is_not_running() || matches!(e, ClientError::Unreachable { .. }),
                "expected NotRunning/Unreachable, got {e:?}"
            ),
            Ok(_) => panic!("connect must fail on a dead socket"),
        }
    }

    #[test]
    fn socket_owned_by_an_unrelated_process_fails_the_handshake_or_transport() {
        // Something accepts connections but speaks no gRPC: the client must
        // fail with a typed error, never hang.
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("imposter.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let accept_thread = std::thread::spawn(move || {
            // Accept one connection and drop it immediately.
            let _ = listener.accept();
        });

        let result = Client::connect(ClientOptions {
            socket: Some(socket),
            connect_timeout: Some(Duration::from_millis(500)),
            request_timeout: Some(Duration::from_millis(500)),
        });
        assert!(result.is_err(), "an imposter socket must not handshake");
        let _ = accept_thread.join();
    }

    #[test]
    fn error_display_names_both_protocol_versions() {
        let err = ClientError::VersionMismatch {
            client_protocol: 1,
            engine_protocol: 2,
            client_version: "0.1.0".into(),
            engine_version: "0.9.0".into(),
        };
        let text = err.to_string();
        assert!(text.contains("protocol v2"), "{text}");
        assert!(text.contains("protocol v1"), "{text}");
        assert!(text.contains("0.9.0"), "{text}");
    }
}

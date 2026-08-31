use crate::admin::AdminService;
use crate::issuer::IssuerService;
use crate::{ClientError, ClientOptions, EngineInfo};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint, Uri};
use valqeron_engine_proto::PROTOCOL_VERSION;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_ENGINE_ENDPOINT: &str = "http://valqeron.engine";

pub const SOCKET_ENV: &str = "VALQERON_SOCKET";

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    /// Resolves the socket path from the `VALQERON_SOCKET` environment variable.
    pub fn resolve() -> Result<Self, ClientError> {
        Self::resolve_with(env::var_os(SOCKET_ENV))
    }

    pub fn exists() -> Result<bool, ClientError> {
        let path = Self::resolve()?;
        Ok(path.0.exists())
    }

    /// Pure resolution core: env value is injected so tests never touch
    /// process-global environment state.
    fn resolve_with(env_value: Option<OsString>) -> Result<Self, ClientError> {
        if let Some(value) = env_value {
            if !value.is_empty() {
                return Ok(Self(PathBuf::from(value)));
            }
        }

        Err(ClientError::Config(format!(
            "could not determine socket path; set {SOCKET_ENV}"
        )))
    }
}

impl From<SocketPath> for PathBuf {
    fn from(socket: SocketPath) -> Self {
        socket.0
    }
}

/// Async-native core shared by both `crate::AsyncClient` and `crate::Client`.
/// Not exposed directly — reach its services through `admin()` / `issuers()`
/// on one of those two.
pub(crate) struct Inner {
    channel: Channel,
    socket: PathBuf,
    engine_info: EngineInfo,
}

impl Inner {
    pub(crate) async fn connect(options: ClientOptions) -> Result<Self, ClientError> {
        let socket: PathBuf = match options.socket {
            Some(path) => path,
            None => SocketPath::resolve()?.into(),
        };

        let connect_timeout = options.connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT);
        let request_timeout = options
            .request_timeout
            .unwrap_or(DEFAULT_RPC_REQUEST_TIMEOUT);

        let endpoint = Endpoint::try_from(DEFAULT_ENGINE_ENDPOINT)
            .map_err(|e| ClientError::Config(e.to_string()))?
            .connect_timeout(connect_timeout)
            .timeout(request_timeout);

        let dial_path = socket.clone();
        let connector = tower::service_fn(move |_uri: Uri| {
            let dial_path = dial_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(dial_path).await?;
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
            }
        });

        let channel = endpoint
            .connect_with_connector(connector)
            .await
            .map_err(|e| classify_connect_error(&socket, &e))?;

        let mut inner = Self {
            channel,
            socket,
            engine_info: EngineInfo {
                engine_version: String::new(),
                protocol_version: 0,
            },
        };
        inner.engine_info = inner.handshake().await?;
        Ok(inner)
    }

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }

    pub(crate) fn engine_info(&self) -> &EngineInfo {
        &self.engine_info
    }

    /// A fresh, cheaply-constructed handle to the engine's admin RPCs.
    pub(crate) fn admin(&self) -> AdminService {
        AdminService::new(self.channel.clone(), self.socket.clone())
    }

    /// A fresh, cheaply-constructed handle to the engine's issuer RPCs.
    pub(crate) fn issuers(&self) -> IssuerService {
        IssuerService::new(self.channel.clone(), self.socket.clone())
    }

    // Add one accessor per new gRPC service here, e.g.:
    // pub(crate) fn securities(&self) -> SecurityService {
    //     SecurityService::new(self.channel.clone(), self.socket.clone())
    // }

    async fn handshake(&self) -> Result<EngineInfo, ClientError> {
        // Reuses AdminService::health rather than duplicating the RPC call.
        let health = self.admin().health().await?;
        if health.protocol_version != PROTOCOL_VERSION {
            return Err(ClientError::VersionMismatch {
                client_protocol: PROTOCOL_VERSION,
                engine_protocol: health.protocol_version,
                client_version: env!("CARGO_PKG_VERSION").to_string(),
                engine_version: health.engine_version,
            });
        }
        Ok(health)
    }
}

fn classify_connect_error(socket: &Path, err: &tonic::transport::Error) -> ClientError {
    use std::error::Error as _;
    let mut source: Option<&(dyn std::error::Error + 'static)> = err.source();
    while let Some(current) = source {
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) {
                return ClientError::NotRunning {
                    socket: socket.to_path_buf(),
                };
            }
            break;
        }
        source = current.source();
    }
    ClientError::Unreachable {
        socket: socket.to_path_buf(),
        message: err.to_string(),
    }
}

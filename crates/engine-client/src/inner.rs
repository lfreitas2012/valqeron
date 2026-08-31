use crate::admin::AdminService;
use crate::issuer::IssuerService;
use crate::{ClientError, ClientOptions, EngineInfo};
use directories::ProjectDirs;
use hyper_util::rt::TokioIo;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint, Uri};
use valqeron_engine_proto::PROTOCOL_VERSION;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_RPC_SOCKET_FILE_NAME: &str = "valqeron.sock";
pub const DEFAULT_ENGINE_ENDPOINT: &str = "http://[::1]:50051";
pub const DEFAULT_VALQERON_QUALIFIER: &str = "io";
pub const DEFAULT_VALQERON_ORGANIZATION: &str = "valqeron";
pub const DEFAULT_VALQERON_APP: &str = "valqeron";

pub const SOCKET_ENV: &str = "VALQERON_SOCKET";

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SocketPath(pub(crate) PathBuf);

impl SocketPath {
    pub fn resolve() -> Result<Self, ClientError> {
        Self::resolve_with(env::var_os(SOCKET_ENV))
    }

    pub fn exists() -> Result<bool, ClientError> {
        let path = Self::resolve()?;
        Ok(path.0.exists())
    }

    fn resolve_with(env_value: Option<OsString>) -> Result<Self, ClientError> {
        if let Some(value) = env_value {
            if !value.is_empty() {
                return Ok(Self(PathBuf::from(value)));
            }
        }

        resolve_default_project_dir()
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .map(|dir| Self(dir.join(DEFAULT_RPC_SOCKET_FILE_NAME)))
            .ok_or(ClientError::Config(format!(
                "could not determine socket path; set {SOCKET_ENV}"
            )))
    }
}

fn resolve_default_project_dir() -> Option<ProjectDirs> {
    ProjectDirs::from(
        DEFAULT_VALQERON_QUALIFIER,
        DEFAULT_VALQERON_ORGANIZATION,
        DEFAULT_VALQERON_APP,
    )
}

impl From<SocketPath> for PathBuf {
    fn from(socket: SocketPath) -> Self {
        socket.0
    }
}

pub(crate) struct Inner {
    channel: Channel,
    socket: PathBuf,
    engine_info: EngineInfo,
}

impl Inner {
    pub(crate) async fn connect(options: ClientOptions) -> Result<Self, ClientError> {
        let socket: PathBuf = match options.socket {
            Some(path) => path.into(),
            None => SocketPath::resolve()?.into(),
        };

        let endpoint = Endpoint::try_from(DEFAULT_ENGINE_ENDPOINT)
            .map_err(|e| ClientError::Config(format!("invalid endpoint: {e}")))?
            .connect_timeout(options.connect_timeout)
            .timeout(options.request_timeout);

        let dial_path = socket.clone();

        let connector = tower::service_fn(move |_uri: Uri| {
            let dial_path = dial_path.clone();

            async move {
                let stream = tokio::net::UnixStream::connect(&dial_path)
                    .await
                    .map_err(|e| {
                        std::io::Error::new(
                            e.kind(),
                            format!(
                                "connecting to Valqeron engine socket {}: {e}",
                                dial_path.display()
                            ),
                        )
                    })?;

                // Simplified using the import at the top of your file
                Ok::<_, std::io::Error>(TokioIo::new(stream))
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

    pub(crate) fn admin(&self) -> AdminService {
        AdminService::new(self.channel.clone(), self.socket.clone())
    }

    pub(crate) fn issuers(&self) -> IssuerService {
        IssuerService::new(self.channel.clone(), self.socket.clone())
    }

    async fn handshake(&self) -> Result<EngineInfo, ClientError> {
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
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);

    while let Some(current) = source {
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            return match io.kind() {
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound => {
                    ClientError::NotRunning {
                        socket: socket.to_path_buf(),
                    }
                }

                _ => ClientError::Unreachable {
                    socket: socket.to_path_buf(),
                    message: io.to_string(),
                },
            };
        }

        source = current.source();
    }

    ClientError::Unreachable {
        socket: socket.to_path_buf(),
        message: err.to_string(),
    }
}

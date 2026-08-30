use crate::grpc::PROTOCOL_VERSION;
use crate::grpc::v1::HealthRequestProto;
use crate::grpc::v1::rpc_admin_service_client::RpcAdminServiceClient;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint, Uri};

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

    #[error("rpc failed ({code}): {message}")]
    Rpc { code: String, message: String },
}

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct EngineInfo {
    pub engine_version: String,
    pub protocol_version: u32,
}

struct Inner {
    channel: Channel,
    socket: PathBuf,
    endpoint: Endpoint,
    engine_info: Option<EngineInfo>,
}

impl Inner {
    async fn connect(mut self) -> Result<Self, ClientError> {
        let dial_path = self.socket.clone();
        let connector = tower::service_fn(move |_uri: Uri| {
            let dial_path = dial_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(dial_path).await?;
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
            }
        });

        let channel = self
            .endpoint
            .connect_with_connector(connector)
            .await
            .map_err(|e| classify_connect_error(&self.socket, &e))?;

        self.engine_info = Some(self.handshake().await?);

        Ok(Self {
            channel,
            socket: self.socket,
            endpoint: self.endpoint,
            engine_info: self.engine_info,
        })
    }

    async fn handshake(&self) -> Result<EngineInfo, ClientError> {
        let health = self.health().await?;
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

    async fn health(&self) -> Result<EngineInfo, ClientError> {
        let mut admin = RpcAdminServiceClient::new(self.channel.clone());
        let response = admin
            .health(HealthRequestProto {})
            .await
            .map_err(|s| self.map_status(s))?
            .into_inner();
        Ok(EngineInfo {
            engine_version: response.engine_version,
            protocol_version: response.protocol_version,
        })
    }

    fn map_status(&self, status: tonic::Status) -> ClientError {
        match status.code() {
            tonic::Code::Unavailable => ClientError::Unreachable {
                socket: self.socket.clone(),
                message: status.message().to_string(),
            },
            code => ClientError::Rpc {
                code: format!("{code:?}"),
                message: status.message().to_string(),
            },
        }
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

use crate::{BackgroundTaskDetail, ClientError, EngineInfo, EngineStatus, map_status};
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::time::SystemTime;
use tonic::transport::Channel;
use uuid::Uuid;
use valqeron_engine_proto::v1::rpc_admin_service_client::RpcAdminServiceClient;
use valqeron_engine_proto::v1::{
    HealthRequestProto, ListBackgroundTasksRequestProto, StatusRequestProto,
};

/// Engine-wide operations that aren't scoped to any one domain: health,
/// version/status, and background task inspection. Reach it via
/// `client.admin()`.
#[derive(Clone)]
pub struct AdminService {
    channel: Channel,
    socket: PathBuf,
}

impl AdminService {
    pub(crate) fn new(channel: Channel, socket: PathBuf) -> Self {
        Self { channel, socket }
    }

    pub async fn health(&self) -> Result<EngineInfo, ClientError> {
        let mut client = RpcAdminServiceClient::new(self.channel.clone());
        let response = client
            .health(HealthRequestProto {})
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        Ok(EngineInfo {
            engine_version: response.engine_version,
            protocol_version: response.protocol_version,
        })
    }

    pub async fn status(&self) -> Result<EngineStatus, ClientError> {
        let mut client = RpcAdminServiceClient::new(self.channel.clone());
        let response = client
            .status(StatusRequestProto {})
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        Ok(EngineStatus {
            engine_version: response.engine_version,
            protocol_version: response.protocol_version,
            pid: response.pid,
        })
    }

    pub async fn list_background_tasks(&self) -> Result<Vec<BackgroundTaskDetail>, ClientError> {
        let mut client = RpcAdminServiceClient::new(self.channel.clone());
        let response = client
            .list_background_tasks(ListBackgroundTasksRequestProto {})
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();

        response
            .tasks
            .into_iter()
            .map(|t| {
                let id = Uuid::from_slice(&t.id)
                    .map_err(|e| ClientError::InvalidResponse(format!("Malformed UUID: {e}")))?;
                let proto_ts = t.created_at.ok_or_else(|| {
                    ClientError::InvalidResponse("Missing created_at timestamp".to_string())
                })?;
                let system_time = SystemTime::try_from(proto_ts).map_err(|e| {
                    ClientError::InvalidResponse(format!("Invalid timestamp range: {e}"))
                })?;
                let created_at: DateTime<Utc> = system_time.into();
                Ok(BackgroundTaskDetail {
                    id,
                    name: t.name,
                    created_at,
                })
            })
            .collect()
    }
}

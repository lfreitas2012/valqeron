use crate::{BackgroundTaskDetail, ClientError, EngineInfo, EngineStatus, map_status};
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::time::SystemTime;
use tonic::transport::Channel;
use uuid::Uuid;
use valqeron_core::common::UniqueIdentifier;
use valqeron_engine_proto::v1::rpc_admin_service_client::RpcAdminServiceClient;
use valqeron_engine_proto::v1::{
    BackgroundTaskDetailProto, HealthRequestProto, ListBackgroundTasksRequestProto,
    PaginatedCursor, StatusRequestProto,
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

    pub async fn list_background_tasks(
        &self,
        after: Option<&UniqueIdentifier>,
        limit: u32,
    ) -> Result<Vec<BackgroundTaskDetail>, ClientError> {
        let request = ListBackgroundTasksRequestProto {
            cursor: Some(PaginatedCursor {
                after: after.map(UniqueIdentifier::value),
                limit,
            }),
        };
        let mut client = RpcAdminServiceClient::new(self.channel.clone());

        let response = client
            .list_background_tasks(request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();

        response
            .tasks
            .into_iter()
            .map(|t| background_task_from_proto(&t))
            .collect()
    }
}

fn background_task_from_proto(
    msg: &BackgroundTaskDetailProto,
) -> Result<BackgroundTaskDetail, ClientError> {
    let id = Uuid::from_slice(&msg.id)
        .map_err(|e| ClientError::InvalidResponse(format!("Malformed UUID: {e}")))?;
    let proto_created_at = msg.created_at.as_ref().ok_or_else(|| {
        ClientError::InvalidResponse("Missing created_at timestamp".to_string())
    })?;
    let system_time_created = SystemTime::try_from(proto_created_at.clone()).map_err(|e| {
        ClientError::InvalidResponse(format!("Invalid timestamp range for created_at: {e}"))
    })?;
    let created_at: DateTime<Utc> = system_time_created.into();

    let last_updated_at = match &msg.last_updated_at {
        Some(proto_ts) => {
            let system_time_updated = SystemTime::try_from(proto_ts.clone()).map_err(|e| {
                ClientError::InvalidResponse(format!(
                    "Invalid timestamp range for last_updated_at: {e}"
                ))
            })?;
            system_time_updated.into()
        }
        None => created_at,
    };

    Ok(BackgroundTaskDetail {
        id,
        name: msg.name.clone(),
        created_at,
        last_updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_background_task_from_proto_valid() {
        let task_id = Uuid::now_v7();
        let proto = BackgroundTaskDetailProto {
            id: task_id.as_bytes().to_vec(),
            name: "sample_task".to_string(),
            created_at: Some(valqeron_engine_proto::prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            last_updated_at: Some(valqeron_engine_proto::prost_types::Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            }),
        };

        let result = background_task_from_proto(&proto).expect("should convert");
        assert_eq!(result.id, task_id);
        assert_eq!(result.name, "sample_task");
        assert_eq!(result.created_at, Utc.timestamp_opt(1_700_000_000, 0).unwrap());
        assert_eq!(
            result.last_updated_at,
            Utc.timestamp_opt(1_700_000_100, 0).unwrap()
        );
    }

    #[test]
    fn test_background_task_from_proto_missing_created_at() {
        let task_id = Uuid::now_v7();
        let proto = BackgroundTaskDetailProto {
            id: task_id.as_bytes().to_vec(),
            name: "sample_task".to_string(),
            created_at: None,
            last_updated_at: None,
        };

        let result = background_task_from_proto(&proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_background_task_from_proto_malformed_uuid() {
        let proto = BackgroundTaskDetailProto {
            id: vec![1, 2, 3],
            name: "sample_task".to_string(),
            created_at: Some(valqeron_engine_proto::prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            last_updated_at: None,
        };

        let result = background_task_from_proto(&proto);
        assert!(result.is_err());
    }
}
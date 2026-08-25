use crate::{Client, ClientError};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::SystemTime;
use uuid::Uuid;
use valqeron_proto::v1::ListBackgroundTasksRequestProto;
use valqeron_proto::v1::rpc_admin_service_client::RpcAdminServiceClient;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackgroundTaskDetail {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub fn list_background_tasks_command(
    client: &Client,
) -> Result<Vec<BackgroundTaskDetail>, ClientError> {
    let mut admin = RpcAdminServiceClient::new(client.channel.clone());
    let response = client
        .runtime
        .block_on(async move {
            admin
                .list_background_tasks(ListBackgroundTasksRequestProto {})
                .await
        })
        .map_err(|s| client.map_status(s))?
        .into_inner();

    response
        .tasks
        .into_iter()
        .map(|t| {
            // Error out if the UUID bytes are not exactly 16 bytes or are invalid
            let id = Uuid::from_slice(&t.id)
                .map_err(|e| ClientError::InvalidResponse(format!("Malformed UUID: {e}")))?;

            // Error out if the engine sent a payload missing the created_at timestamp
            let proto_ts = t.created_at.ok_or_else(|| {
                ClientError::InvalidResponse("Missing created_at timestamp".to_string())
            })?;

            // Error out if the timestamp cannot be safely parsed into a SystemTime
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

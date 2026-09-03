use crate::grpc::ValqeronEngineGrpc;
use tonic::{Request, Response, Status};
use uuid::Uuid;
use valqeron_core::BackgroundTask;
use valqeron_core::common::UniqueIdentifier;
use valqeron_engine_proto::PROTOCOL_VERSION;
use valqeron_engine_proto::v1::rpc_admin_service_server::RpcAdminService;
use valqeron_engine_proto::v1::{
    BackgroundTaskDetailProto, HealthRequestProto, HealthResponseProto,
    ListBackgroundTasksRequestProto, ListBackgroundTasksResponseProto, PaginatedCursor,
    StatusRequestProto, StatusResponseProto,
};

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 1_000;

#[tonic::async_trait]
impl RpcAdminService for ValqeronEngineGrpc {
    async fn health(
        &self,
        _request: Request<HealthRequestProto>,
    ) -> Result<Response<HealthResponseProto>, Status> {
        Ok(Response::new(HealthResponseProto {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
        }))
    }

    async fn status(
        &self,
        _request: Request<StatusRequestProto>,
    ) -> Result<Response<StatusResponseProto>, Status> {
        Ok(Response::new(StatusResponseProto {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            pid: std::process::id(),
        }))
    }

    async fn list_background_tasks(
        &self,
        request: Request<ListBackgroundTasksRequestProto>,
    ) -> Result<Response<ListBackgroundTasksResponseProto>, Status> {
        let req = request.into_inner();
        let (after, limit) = parse_cursor(req.cursor)?;

        let tasks = self
            .run_read("admin.list_background_tasks", move |repos| {
                valqeron_core::tasks::list_background_tasks(
                    &repos.background_task_definition,
                    after,
                    limit,
                )
                .map_err(|e| Status::internal(e.to_string()))
            })
            .await?;

        let next_cursor = tasks.last().map(|task| PaginatedCursor {
            after: Some(task.data.id().value()),
            limit,
        });

        let task_protos = tasks
            .into_iter()
            .map(|t| convert_background_task_to_proto(&t.data))
            .collect();

        Ok(Response::new(ListBackgroundTasksResponseProto {
            cursor: next_cursor,
            protocol_version: PROTOCOL_VERSION,
            tasks: task_protos,
        }))
    }
}

fn convert_background_task_to_proto(task: &BackgroundTask) -> BackgroundTaskDetailProto {
    BackgroundTaskDetailProto {
        id: task.id().as_bytes().to_vec(),
        name: task.name().as_str().to_string(),
        created_at: Some(valqeron_engine_proto::prost_types::Timestamp {
            seconds: task.created_at().timestamp(),
            nanos: task.created_at().timestamp_subsec_nanos() as i32,
        }),
        last_updated_at: Some(valqeron_engine_proto::prost_types::Timestamp {
            seconds: task.last_updated_at().timestamp(),
            nanos: task.last_updated_at().timestamp_subsec_nanos() as i32,
        }),
    }
}

fn parse_cursor(
    cursor: Option<PaginatedCursor>,
) -> Result<(Option<UniqueIdentifier>, u32), Status> {
    let cursor = cursor.unwrap_or_default();
    let after = cursor
        .after
        .map(|raw| {
            Uuid::parse_str(&raw)
                .map(UniqueIdentifier::from_uuid)
                .map_err(|e| Status::invalid_argument(format!("invalid cursor after id: {e}")))
        })
        .transpose()?;

    let limit = match cursor.limit {
        0 => DEFAULT_LIST_LIMIT,
        l => l.min(MAX_LIST_LIMIT),
    };

    Ok((after, limit))
}

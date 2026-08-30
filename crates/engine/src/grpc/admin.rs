use crate::grpc::v1::rpc_admin_service_server::RpcAdminService;
use crate::grpc::v1::{
    HealthRequestProto, HealthResponseProto, ListBackgroundTasksRequestProto,
    ListBackgroundTasksResponseProto, StatusRequestProto, StatusResponseProto,
};
use crate::grpc::{PROTOCOL_VERSION, ValqeronEngineGrpc};
use tonic::{Request, Response, Status};

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

        // TODO: implement this

        Ok(Response::new(ListBackgroundTasksResponseProto {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            tasks: vec![],
        }))
    }
}

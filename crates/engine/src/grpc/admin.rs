use std::time::Instant;

use tonic::{Request, Response, Status};
use v1::{HealthRequestProto, HealthResponseProto, StatusRequestProto, StatusResponseProto};
use valqeron_proto::PROTOCOL_VERSION;
use valqeron_proto::v1;
use valqeron_proto::v1::rpc_admin_service_server::RpcAdminService;

pub struct AdminGrpc {
    db_path: String,
    started: Instant,
}

impl AdminGrpc {
    pub fn new(db_path: String, started: Instant) -> Self {
        Self { db_path, started }
    }
}

#[tonic::async_trait]
impl RpcAdminService for AdminGrpc {
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
            db_path: self.db_path.clone(),
            uptime_secs: self.started.elapsed().as_secs(),
            pid: std::process::id(),
        }))
    }
}

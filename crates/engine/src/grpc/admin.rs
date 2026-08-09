use std::time::Instant;

use tonic::{Request, Response, Status};
use valqeron_proto::PROTOCOL_VERSION;
use valqeron_proto::v1;
use valqeron_proto::v1::admin_service_server::AdminService;

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
impl AdminService for AdminGrpc {
    async fn health(
        &self,
        _request: Request<v1::HealthRequest>,
    ) -> Result<Response<v1::HealthResponse>, Status> {
        Ok(Response::new(v1::HealthResponse {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
        }))
    }

    async fn status(
        &self,
        _request: Request<v1::StatusRequest>,
    ) -> Result<Response<v1::StatusResponse>, Status> {
        Ok(Response::new(v1::StatusResponse {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            db_path: self.db_path.clone(),
            uptime_secs: self.started.elapsed().as_secs(),
            pid: std::process::id(),
        }))
    }
}

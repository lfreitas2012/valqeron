use crate::grpc::ValqeronEngineGrpc;
use crate::grpc::v1::rpc_issuer_service_server::RpcIssuerService;
use crate::grpc::v1::{
    DeleteIssuerRequestProto, DeleteIssuerResponseProto, GetIssuerRequestProto,
    GetIssuerResponseProto, ListIssuersRequestProto, ListIssuersResponseProto,
    PatchIssuerRequestProto, PatchIssuerResponseProto, RegisterIssuerRequestProto,
    RegisterIssuerResponseProto,
};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl RpcIssuerService for ValqeronEngineGrpc {
    async fn register(
        &self,
        request: Request<RegisterIssuerRequestProto>,
    ) -> Result<Response<RegisterIssuerResponseProto>, Status> {
        todo!()
    }

    async fn get(
        &self,
        request: Request<GetIssuerRequestProto>,
    ) -> Result<Response<GetIssuerResponseProto>, Status> {
        todo!()
    }

    async fn list(
        &self,
        request: Request<ListIssuersRequestProto>,
    ) -> Result<Response<ListIssuersResponseProto>, Status> {
        todo!()
    }

    async fn patch(
        &self,
        request: Request<PatchIssuerRequestProto>,
    ) -> Result<Response<PatchIssuerResponseProto>, Status> {
        todo!()
    }

    async fn delete(
        &self,
        request: Request<DeleteIssuerRequestProto>,
    ) -> Result<Response<DeleteIssuerResponseProto>, Status> {
        todo!()
    }
}

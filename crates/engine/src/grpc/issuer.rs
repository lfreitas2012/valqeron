use tonic::{Request, Response, Status};
use v1::{
    DeleteIssuerRequestProto, DeleteIssuerResponseProto, IssuerProto, ListIssuersResponseProto,
    PatchIssuerResponseProto,
};
use valqeron_core::{IssuerRepository, LoadMode, StorageEngine, Versioned, register_issuer};
use valqeron_proto::v1::rpc_issuer_service_server::RpcIssuerService;
use valqeron_proto::{
    issuer_to_proto, parse_issuer_id, patch_request_to_domain, register_request_to_issuer, v1,
    write_outcome_to_proto,
};

use crate::grpc::problem::HandlerError;
use crate::storage::AsyncStorage;

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 1_000;

pub struct IssuerGrpc {
    storage: AsyncStorage,
}

impl IssuerGrpc {
    pub fn new(storage: AsyncStorage) -> Self {
        Self { storage }
    }

    async fn run<T, F>(&self, operation: &'static str, f: F) -> Result<T, Status>
    where
        F: FnOnce(&valqeron_infrastructure::SqliteStorageEngine) -> Result<T, HandlerError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        match self.storage.call(operation, f).await {
            Ok(inner) => inner.map_err(HandlerError::into_status),
            Err(call_err) => Err(HandlerError::from(call_err).into_status()),
        }
    }
}

#[tonic::async_trait]
impl RpcIssuerService for IssuerGrpc {
    async fn register(
        &self,
        request: Request<v1::RegisterIssuerRequestProto>,
    ) -> Result<Response<v1::RegisterIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let dry_run = req.dry_run;
        let issuer =
            register_request_to_issuer(&req).map_err(|e| HandlerError::from(e).into_status())?;

        let registered = self
            .run("issuer.register", move |engine| {
                let outcome = if dry_run {
                    match engine.dry_run(|repos| register_issuer(&repos.issuers, &issuer)) {
                        Ok(inner) => inner.map_err(HandlerError::from),
                        Err(storage_err) => Err(HandlerError::from(storage_err)),
                    }
                } else {
                    register_issuer(&engine.repositories().issuers, &issuer)
                        .map_err(HandlerError::from)
                };
                outcome.map(|()| {
                    issuer_to_proto(&Versioned {
                        data: issuer,
                        version: 1,
                    })
                })
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.register",
            id = %registered.id,
            dry_run,
            "registered issuer"
        );

        Ok(Response::new(v1::RegisterIssuerResponseProto {
            issuer: Some(registered),
        }))
    }

    async fn get(
        &self,
        request: Request<v1::GetIssuerRequestProto>,
    ) -> Result<Response<v1::GetIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let id = parse_issuer_id(&req.id).map_err(|e| HandlerError::from(e).into_status())?;

        let issuer = self
            .run("issuer.get", move |engine| {
                engine
                    .repositories()
                    .issuers
                    .find_by_id(&id, LoadMode::Lazy)
                    .map(|found| found.as_ref().map(issuer_to_proto))
                    .map_err(HandlerError::from)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.info",
            id = %req.id,
            found = issuer.is_some(),
            "issuer lookup"
        );

        Ok(Response::new(v1::GetIssuerResponseProto { issuer }))
    }

    async fn list(
        &self,
        request: Request<v1::ListIssuersRequestProto>,
    ) -> Result<Response<ListIssuersResponseProto>, Status> {
        let req = request.into_inner();
        let after = match req.after.as_deref() {
            Some(raw) => {
                Some(parse_issuer_id(raw).map_err(|e| HandlerError::from(e).into_status())?)
            }
            None => None,
        };
        let limit = if req.limit == 0 {
            DEFAULT_LIST_LIMIT
        } else {
            req.limit.min(MAX_LIST_LIMIT)
        };

        let issuers: Vec<IssuerProto> = self
            .run("issuer.list", move |engine| {
                engine
                    .repositories()
                    .issuers
                    .list_paged(after, limit, LoadMode::Lazy)
                    .map(|rows| rows.iter().map(issuer_to_proto).collect())
                    .map_err(HandlerError::from)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.list",
            count = issuers.len(),
            limit,
            "list registered issuers (paged)"
        );

        Ok(Response::new(ListIssuersResponseProto { issuers }))
    }

    async fn patch(
        &self,
        request: Request<v1::PatchIssuerRequestProto>,
    ) -> Result<Response<PatchIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let cmd = patch_request_to_domain(&req).map_err(|e| HandlerError::from(e).into_status())?;

        let outcome = self
            .run("issuer.patch", move |engine| {
                let result = if cmd.dry_run {
                    match engine.dry_run(|repos| {
                        repos
                            .issuers
                            .apply_patch(&cmd.id, cmd.expected_version, cmd.patch.clone())
                    }) {
                        Ok(inner) => inner.map_err(HandlerError::from),
                        Err(storage_err) => Err(HandlerError::from(storage_err)),
                    }
                } else {
                    engine
                        .repositories()
                        .issuers
                        .apply_patch(&cmd.id, cmd.expected_version, cmd.patch.clone())
                        .map_err(HandlerError::from)
                };
                result.map(write_outcome_to_proto)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.patch",
            id = %req.id,
            dry_run = req.dry_run,
            "patched issuer"
        );

        Ok(Response::new(PatchIssuerResponseProto {
            outcome: Some(outcome),
        }))
    }

    async fn delete(
        &self,
        request: Request<DeleteIssuerRequestProto>,
    ) -> Result<Response<DeleteIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let id = parse_issuer_id(&req.id).map_err(|e| HandlerError::from(e).into_status())?;
        let expected_version = req.expected_version;
        let dry_run = req.dry_run;

        let outcome = self
            .run("issuer.delete", move |engine| {
                let result = if dry_run {
                    match engine.dry_run(|repos| repos.issuers.delete(&id, expected_version)) {
                        Ok(inner) => inner.map_err(HandlerError::from),
                        Err(storage_err) => Err(HandlerError::from(storage_err)),
                    }
                } else {
                    engine
                        .repositories()
                        .issuers
                        .delete(&id, expected_version)
                        .map_err(HandlerError::from)
                };
                result.map(write_outcome_to_proto)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.delete",
            id = %req.id,
            dry_run,
            "deleted issuer"
        );

        Ok(Response::new(DeleteIssuerResponseProto {
            outcome: Some(outcome),
        }))
    }
}

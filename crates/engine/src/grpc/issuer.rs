use tonic::{Request, Response, Status};
use valqeron_core::{IssuerRepository, LoadMode, StorageEngine, Versioned, register_issuer};
use valqeron_proto::v1::issuer_service_server::IssuerService;
use valqeron_proto::{mapping, v1};

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
impl IssuerService for IssuerGrpc {
    async fn register(
        &self,
        request: Request<v1::RegisterIssuerRequest>,
    ) -> Result<Response<v1::RegisterIssuerResponse>, Status> {
        let req = request.into_inner();
        let dry_run = req.dry_run;
        let issuer = mapping::register_request_to_issuer(&req)
            .map_err(|e| HandlerError::from(e).into_status())?;

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
                    mapping::issuer_to_proto(&Versioned {
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

        Ok(Response::new(v1::RegisterIssuerResponse {
            issuer: Some(registered),
        }))
    }

    async fn get(
        &self,
        request: Request<v1::GetIssuerRequest>,
    ) -> Result<Response<v1::GetIssuerResponse>, Status> {
        let req = request.into_inner();
        let id =
            mapping::parse_issuer_id(&req.id).map_err(|e| HandlerError::from(e).into_status())?;

        let issuer = self
            .run("issuer.get", move |engine| {
                engine
                    .repositories()
                    .issuers
                    .find_by_id(&id, LoadMode::Lazy)
                    .map(|found| found.as_ref().map(mapping::issuer_to_proto))
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

        Ok(Response::new(v1::GetIssuerResponse { issuer }))
    }

    async fn list(
        &self,
        request: Request<v1::ListIssuersRequest>,
    ) -> Result<Response<v1::ListIssuersResponse>, Status> {
        let req = request.into_inner();
        let after = match req.after.as_deref() {
            Some(raw) => Some(
                mapping::parse_issuer_id(raw).map_err(|e| HandlerError::from(e).into_status())?,
            ),
            None => None,
        };
        let limit = if req.limit == 0 {
            DEFAULT_LIST_LIMIT
        } else {
            req.limit.min(MAX_LIST_LIMIT)
        };

        let issuers: Vec<v1::Issuer> = self
            .run("issuer.list", move |engine| {
                engine
                    .repositories()
                    .issuers
                    .list_paged(after, limit, LoadMode::Lazy)
                    .map(|rows| rows.iter().map(mapping::issuer_to_proto).collect())
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

        Ok(Response::new(v1::ListIssuersResponse { issuers }))
    }

    async fn patch(
        &self,
        request: Request<v1::PatchIssuerRequest>,
    ) -> Result<Response<v1::PatchIssuerResponse>, Status> {
        let req = request.into_inner();
        let cmd = mapping::patch_request_to_domain(&req)
            .map_err(|e| HandlerError::from(e).into_status())?;

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
                result.map(mapping::write_outcome_to_proto)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.patch",
            id = %req.id,
            dry_run = req.dry_run,
            "patched issuer"
        );

        Ok(Response::new(v1::PatchIssuerResponse {
            outcome: Some(outcome),
        }))
    }

    async fn delete(
        &self,
        request: Request<v1::DeleteIssuerRequest>,
    ) -> Result<Response<v1::DeleteIssuerResponse>, Status> {
        let req = request.into_inner();
        let id =
            mapping::parse_issuer_id(&req.id).map_err(|e| HandlerError::from(e).into_status())?;
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
                result.map(mapping::write_outcome_to_proto)
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.delete",
            id = %req.id,
            dry_run,
            "deleted issuer"
        );

        Ok(Response::new(v1::DeleteIssuerResponse {
            outcome: Some(outcome),
        }))
    }
}

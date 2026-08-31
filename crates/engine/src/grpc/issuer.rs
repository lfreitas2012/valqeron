use crate::grpc::ValqeronEngineGrpc;
use chrono::{DateTime, Utc};
use std::str::FromStr;
use tonic::{Request, Response, Status};
use valqeron_core::common::Versioned;
use valqeron_core::domain::issuer::{
    Issuer, IssuerBuilderError, IssuerName, IssuerNameError, IssuerStatus, IssuerStatusError,
    RegisterIssuerError, register_issuer,
};
use valqeron_core::identifiers::{Cnpj, CnpjError};
use valqeron_core::{CountryCode, CountryCodeError, Lei, LeiError};
use valqeron_engine_proto::v1::rpc_issuer_service_server::RpcIssuerService;
use valqeron_engine_proto::v1::{
    DeleteIssuerRequestProto, DeleteIssuerResponseProto, GetIssuerRequestProto,
    GetIssuerResponseProto, IssuerProto, ListIssuersRequestProto, ListIssuersResponseProto,
    PatchIssuerRequestProto, PatchIssuerResponseProto, RegisterIssuerRequestProto,
    RegisterIssuerResponseProto,
};

#[tonic::async_trait]
impl RpcIssuerService for ValqeronEngineGrpc {
    async fn register(
        &self,
        request: Request<RegisterIssuerRequestProto>,
    ) -> Result<Response<RegisterIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let dry_run = req.dry_run;
        let issuer = build_issuer_from_proto(&req)?;

        let registered = self
            .run_write(
                "issuer.register",
                dry_run,
                move |repos| -> Result<_, IssuerServiceError> {
                    register_issuer(&repos.issuers, &issuer)?;

                    Ok(convert_versioned_issuer_to_proto(&Versioned {
                        data: issuer,
                        version: 1,
                    }))
                },
            )
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.register",
            id = %registered.id,
            dry_run,
            "registered issuer"
        );

        Ok(Response::new(RegisterIssuerResponseProto {
            issuer: Some(registered),
        }))
    }

    async fn get(
        &self,
        _request: Request<GetIssuerRequestProto>,
    ) -> Result<Response<GetIssuerResponseProto>, Status> {
        todo!()
    }

    async fn list(
        &self,
        _request: Request<ListIssuersRequestProto>,
    ) -> Result<Response<ListIssuersResponseProto>, Status> {
        todo!()
    }

    async fn patch(
        &self,
        _request: Request<PatchIssuerRequestProto>,
    ) -> Result<Response<PatchIssuerResponseProto>, Status> {
        todo!()
    }

    async fn delete(
        &self,
        _request: Request<DeleteIssuerRequestProto>,
    ) -> Result<Response<DeleteIssuerResponseProto>, Status> {
        todo!()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IssuerServiceError {
    #[error(transparent)]
    IssuerName(#[from] IssuerNameError),

    #[error(transparent)]
    IssuerStatus(#[from] IssuerStatusError),

    #[error(transparent)]
    Cnpj(#[from] CnpjError),

    #[error(transparent)]
    Lei(#[from] LeiError),

    #[error(transparent)]
    CountryCode(#[from] CountryCodeError),

    #[error(transparent)]
    IssuerBuilder(#[from] IssuerBuilderError),

    #[error(transparent)]
    RegisterIssuer(#[from] RegisterIssuerError),

    #[error("Storage error: {0}")]
    Storage(#[from] valqeron_core::StorageError),
}

impl From<IssuerServiceError> for Status {
    fn from(err: IssuerServiceError) -> Self {
        match err {
            // Validation errors mapped to 400
            IssuerServiceError::IssuerName(_)
            | IssuerServiceError::IssuerStatus(_)
            | IssuerServiceError::Lei(_)
            | IssuerServiceError::CountryCode(_)
            | IssuerServiceError::IssuerBuilder(_)
            | IssuerServiceError::RegisterIssuer(_)
            | IssuerServiceError::Cnpj(_) => Status::invalid_argument(err.to_string()),

            // Storage errors mapped to 500
            IssuerServiceError::Storage(e) => {
                tracing::error!("Database failure: {:?}", e);
                Status::internal("An internal database error occurred")
            }
        }
    }
}

fn build_register_issuer_request(issuer: &Issuer, dry_run: bool) -> RegisterIssuerRequestProto {
    RegisterIssuerRequestProto {
        name: issuer.name().map(|n| n.as_str().to_string()),
        status: Some(String::from(issuer.status())),
        cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
        lei: issuer.lei().map(|l| l.as_str().to_string()),
        country_code: issuer.country_code().map(|c| c.as_str().to_string()),
        dry_run,
    }
}

fn build_issuer_from_proto(req: &RegisterIssuerRequestProto) -> Result<Issuer, IssuerServiceError> {
    let now = Utc::now();
    let created_at = DateTime::<Utc>::from_timestamp_millis(now.timestamp_millis()).unwrap_or(now);
    let mut builder = Issuer::builder().created_at(created_at);

    if let Some(name) = req.name.as_deref() {
        builder = builder.name(IssuerName::new(name)?);
    }
    if let Some(status) = req.status.as_deref() {
        builder = builder.status(IssuerStatus::from_str(status)?);
    }
    if let Some(cnpj) = req.cnpj.as_deref() {
        builder = builder.cnpj(Cnpj::parse(cnpj)?);
    }
    if let Some(lei) = req.lei.as_deref() {
        builder = builder.lei(Lei::parse(lei)?);
    }
    if let Some(cc) = req.country_code.as_deref() {
        builder = builder.country_code(CountryCode::parse(cc)?);
    }

    Ok(builder.build()?)
}

fn convert_versioned_issuer_to_proto(versioned: &Versioned<Issuer>) -> IssuerProto {
    let issuer = &versioned.data;
    IssuerProto {
        id: issuer.id().value(),
        status: String::from(issuer.status()),
        created_at: issuer.created_at().to_rfc3339(),
        version: versioned.version,
        name: issuer.name().map(|n| n.as_str().to_string()),
        cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
        lei: issuer.lei().map(|l| l.as_str().to_string()),
        country_code: issuer.country_code().map(|c| c.as_str().to_string()),
    }
}

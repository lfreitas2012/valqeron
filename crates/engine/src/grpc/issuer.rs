use crate::grpc::ValqeronEngineGrpc;
use chrono::{DateTime, Utc};
use std::str::FromStr;
use tonic::{Request, Response, Status};
use uuid::Uuid;
use valqeron_core::common::{LoadMode, NonEmpty, Versioned, WriteOutcome};
use valqeron_core::domain::issuer::{
    DeleteIssuerError, GetIssuerError, Issuer, IssuerBuilderError, IssuerId, IssuerName,
    IssuerNameError, IssuerPatch, IssuerPatchBuilder, IssuerStatus, IssuerStatusError,
    ListIssuersError, PatchIssuerError, RegisterIssuerError, delete_issuer, get_issuer,
    list_issuers, patch_issuer, register_issuer,
};
use valqeron_core::identifiers::{Cnpj, CnpjError, CountryCodeError, Lei, LeiError};
use valqeron_core::{CountryCode, StorageError};
use valqeron_engine_proto::v1::rpc_issuer_service_server::RpcIssuerService;
use valqeron_engine_proto::v1::write_outcome_proto::{Applied, Missing, Outcome, VersionMismatch};
use valqeron_engine_proto::v1::{
    DeleteIssuerRequestProto, DeleteIssuerResponseProto, GetIssuerRequestProto,
    GetIssuerResponseProto, IssuerProto, ListIssuersRequestProto, ListIssuersResponseProto,
    PatchIssuerRequestProto, PatchIssuerResponseProto, RegisterIssuerRequestProto,
    RegisterIssuerResponseProto, WriteOutcomeProto,
};

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 1_000;

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
            .run_write("issuer.register", dry_run, move |repos| {
                register_issuer(&repos.issuers, &issuer)?;

                Ok::<_, IssuerServiceError>(convert_versioned_issuer_to_proto(&Versioned {
                    data: issuer,
                    version: 1,
                }))
            })
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
        request: Request<GetIssuerRequestProto>,
    ) -> Result<Response<GetIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let id = parse_issuer_id_to_domain(&req.id)?;

        let issuer = self
            .run_read("issuer.get", move |repos| {
                let found = get_issuer(&repos.issuers, &id, LoadMode::Lazy)?;
                Ok::<_, IssuerServiceError>(found.as_ref().map(convert_versioned_issuer_to_proto))
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.info",
            id = %req.id,
            found = issuer.is_some(),
            "issuer lookup"
        );

        Ok(Response::new(GetIssuerResponseProto { issuer }))
    }

    async fn list(
        &self,
        request: Request<ListIssuersRequestProto>,
    ) -> Result<Response<ListIssuersResponseProto>, Status> {
        let req = request.into_inner();
        let after = match req.after.as_deref() {
            Some(raw) => Some(parse_issuer_id_to_domain(raw)?),
            None => None,
        };
        let limit = if req.limit == 0 {
            DEFAULT_LIST_LIMIT
        } else {
            req.limit.min(MAX_LIST_LIMIT)
        };

        let issuers: Vec<IssuerProto> = self
            .run_read("issuer.list", move |repos| {
                let rows = list_issuers(&repos.issuers, after, limit, LoadMode::Lazy)?;
                Ok::<_, IssuerServiceError>(
                    rows.iter().map(convert_versioned_issuer_to_proto).collect(),
                )
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
        request: Request<PatchIssuerRequestProto>,
    ) -> Result<Response<PatchIssuerResponseProto>, Status> {
        let req = request.into_inner();
        let dry_run = req.dry_run;
        let (id, expected_version, patch) = patch_request_to_domain(&req)?;

        let outcome = self
            .run_write("issuer.patch", dry_run, move |repos| {
                let res = patch_issuer(&repos.issuers, &id, expected_version, patch)?;
                Ok::<_, IssuerServiceError>(write_outcome_to_proto(res))
            })
            .await?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.patch",
            id = %req.id,
            dry_run,
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
        let dry_run = req.dry_run;
        let id = parse_issuer_id_to_domain(&req.id)?;
        let expected_version = req.expected_version;

        let outcome = self
            .run_write("issuer.delete", dry_run, move |repos| {
                let res = delete_issuer(&repos.issuers, &id, expected_version)?;
                Ok::<_, IssuerServiceError>(write_outcome_to_proto(res))
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

#[derive(thiserror::Error, Debug)]
enum MappingError {
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
}

#[derive(thiserror::Error, Debug)]
enum IssuerServiceError {
    #[error(transparent)]
    Register(#[from] RegisterIssuerError),
    #[error(transparent)]
    Get(#[from] GetIssuerError),
    #[error(transparent)]
    List(#[from] ListIssuersError),
    #[error(transparent)]
    Patch(#[from] PatchIssuerError),
    #[error(transparent)]
    Delete(#[from] DeleteIssuerError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl From<IssuerServiceError> for Status {
    fn from(err: IssuerServiceError) -> Self {
        match err {
            IssuerServiceError::Register(RegisterIssuerError::DuplicateCnpj)
            | IssuerServiceError::Register(RegisterIssuerError::DuplicateLei)
            | IssuerServiceError::Patch(PatchIssuerError::DuplicateCnpj)
            | IssuerServiceError::Patch(PatchIssuerError::DuplicateLei) => {
                Status::already_exists(err.to_string())
            }
            _ => {
                tracing::error!(error = %err, "issuer service operation failed");
                Status::internal("an internal storage error occurred")
            }
        }
    }
}

impl From<MappingError> for Status {
    fn from(err: MappingError) -> Self {
        match err {
            MappingError::IssuerName(_)
            | MappingError::IssuerStatus(_)
            | MappingError::Lei(_)
            | MappingError::CountryCode(_)
            | MappingError::IssuerBuilder(_)
            | MappingError::Cnpj(_) => Status::invalid_argument(err.to_string()),
        }
    }
}

fn build_issuer_from_proto(req: &RegisterIssuerRequestProto) -> Result<Issuer, MappingError> {
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

fn parse_issuer_id_to_domain(raw: &str) -> Result<IssuerId, Status> {
    Uuid::parse_str(raw)
        .map(IssuerId::from_uuid)
        .map_err(|_| Status::invalid_argument(format!("Invalid issuer id: {}", raw)))
}

fn patch_request_to_domain(
    req: &PatchIssuerRequestProto,
) -> Result<(IssuerId, u32, IssuerPatch), Status> {
    let id = parse_issuer_id_to_domain(&req.id)?;
    let mut builder: Option<IssuerPatchBuilder<NonEmpty>> = None;

    macro_rules! set_patch_field {
        ($value:expr, $method:ident) => {
            builder = Some(match builder {
                Some(builder) => builder.$method($value),
                None => IssuerPatch::builder().$method($value),
            });
        };
    }

    if let Some(name) = req.name.as_deref() {
        set_patch_field!(IssuerName::new(name).map_err(MappingError::from)?, name);
    }
    if let Some(status) = req.status.as_deref() {
        set_patch_field!(
            IssuerStatus::from_str(status).map_err(MappingError::from)?,
            status
        );
    }
    if let Some(cnpj) = req.cnpj.as_deref() {
        set_patch_field!(Cnpj::parse(cnpj).map_err(MappingError::from)?, cnpj);
    }
    if let Some(lei) = req.lei.as_deref() {
        set_patch_field!(Lei::parse(lei).map_err(MappingError::from)?, lei);
    }
    if let Some(country_code) = req.country_code.as_deref() {
        set_patch_field!(
            CountryCode::parse(country_code).map_err(MappingError::from)?,
            country_code
        );
    }

    let patch = builder
        .ok_or_else(|| Status::invalid_argument("issuer patch must contain at least one field"))?
        .build();
    Ok((id, req.expected_version, patch))
}

fn write_outcome_to_proto(outcome: WriteOutcome) -> WriteOutcomeProto {
    let outcome = match outcome {
        WriteOutcome::Applied => Outcome::Applied(Applied {}),
        WriteOutcome::VersionMismatch { expected, actual } => {
            Outcome::VersionMismatch(VersionMismatch { expected, actual })
        }
        WriteOutcome::Missing => Outcome::Missing(Missing {}),
    };
    WriteOutcomeProto {
        outcome: Some(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_request_requires_at_least_one_field() {
        let request = PatchIssuerRequestProto {
            id: Uuid::now_v7().to_string(),
            expected_version: 1,
            ..Default::default()
        };

        let status = patch_request_to_domain(&request).unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn patch_request_maps_all_fields() {
        let request = PatchIssuerRequestProto {
            id: Uuid::now_v7().to_string(),
            expected_version: 7,
            name: Some("Renamed issuer".to_string()),
            status: Some("RETIRED".to_string()),
            country_code: Some("US".to_string()),
            ..Default::default()
        };

        let (_, expected_version, patch) = patch_request_to_domain(&request).unwrap();
        assert_eq!(expected_version, 7);
        assert_eq!(patch.name().map(IssuerName::as_str), Some("Renamed issuer"));
        assert_eq!(patch.status(), Some(IssuerStatus::Retired));
        assert_eq!(patch.country_code().map(CountryCode::as_str), Some("US"));
    }

    #[test]
    fn write_outcome_maps_version_mismatch() {
        let proto = write_outcome_to_proto(WriteOutcome::VersionMismatch {
            expected: 2,
            actual: 5,
        });

        assert!(matches!(
            proto.outcome,
            Some(Outcome::VersionMismatch(VersionMismatch {
                expected: 2,
                actual: 5
            }))
        ));
    }
}

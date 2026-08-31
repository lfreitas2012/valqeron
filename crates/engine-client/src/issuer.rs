use crate::{ClientError, map_status};
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::str::FromStr;
use tonic::transport::Channel;
use uuid::Uuid;
use valqeron_core::common::{Versioned, WriteOutcome};
use valqeron_core::domain::issuer::{
    Issuer, IssuerBuilderError, IssuerId, IssuerName, IssuerNameError, IssuerPatch, IssuerStatus,
    IssuerStatusError,
};
use valqeron_core::identifiers::{Cnpj, CnpjError};
use valqeron_core::{CountryCode, CountryCodeError, Lei, LeiError};
use valqeron_engine_proto::v1::rpc_issuer_service_client::RpcIssuerServiceClient;
use valqeron_engine_proto::v1::write_outcome_proto::Outcome;
use valqeron_engine_proto::v1::{
    DeleteIssuerRequestProto, GetIssuerRequestProto, IssuerProto, ListIssuersRequestProto,
    PatchIssuerRequestProto, RegisterIssuerRequestProto, WriteOutcomeProto,
};

/// Issuer domain operations. Reach it via `client.issuers()`.
#[derive(Clone)]
pub struct IssuerService {
    channel: Channel,
    socket: PathBuf,
}

impl IssuerService {
    pub(crate) fn new(channel: Channel, socket: PathBuf) -> Self {
        Self { channel, socket }
    }

    pub async fn register(
        &self,
        request: RegisterIssuerRequest,
    ) -> Result<Versioned<Issuer>, ClientError> {
        let proto_request = issuer_to_register_request(&request.issuer, request.dry_run);
        let mut client = RpcIssuerServiceClient::new(self.channel.clone());
        let response = client
            .register(proto_request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        let wire = response
            .issuer
            .ok_or_else(|| ClientError::InvalidResponse("register returned no issuer".into()))?;
        issuer_from_proto(&wire).map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }

    pub async fn get(&self, id: &IssuerId) -> Result<Option<Versioned<Issuer>>, ClientError> {
        let request = GetIssuerRequestProto { id: id.value() };
        let mut client = RpcIssuerServiceClient::new(self.channel.clone());
        let response = client
            .get(request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        response
            .issuer
            .as_ref()
            .map(issuer_from_proto)
            .transpose()
            .map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }

    pub async fn list(
        &self,
        after: Option<&IssuerId>,
        limit: u32,
    ) -> Result<Vec<Versioned<Issuer>>, ClientError> {
        let request = ListIssuersRequestProto {
            after: after.map(IssuerId::value),
            limit,
        };
        let mut client = RpcIssuerServiceClient::new(self.channel.clone());
        let response = client
            .list(request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        response
            .issuers
            .iter()
            .map(issuer_from_proto)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }

    pub async fn patch(
        &self,
        id: &IssuerId,
        request: PatchIssuerRequest,
    ) -> Result<WriteOutcome, ClientError> {
        let proto_request = PatchIssuerRequestProto {
            id: id.value(),
            expected_version: request.expected_version,
            name: request.patch.name().map(|n| n.as_str().to_string()),
            status: request.patch.status().map(String::from),
            cnpj: request.patch.cnpj().map(|c| c.as_str().to_string()),
            lei: request.patch.lei().map(|l| l.as_str().to_string()),
            country_code: request.patch.country_code().map(|c| c.as_str().to_string()),
            dry_run: request.dry_run,
        };
        let mut client = RpcIssuerServiceClient::new(self.channel.clone());
        let response = client
            .patch(proto_request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        let outcome = response
            .outcome
            .ok_or_else(|| ClientError::InvalidResponse("patch returned no outcome".into()))?;
        write_outcome_from_proto(&outcome).map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }

    pub async fn delete(
        &self,
        id: &IssuerId,
        request: DeleteIssuerRequest,
    ) -> Result<WriteOutcome, ClientError> {
        let proto_request = DeleteIssuerRequestProto {
            id: id.value(),
            expected_version: request.expected_version,
            dry_run: request.dry_run,
        };
        let mut client = RpcIssuerServiceClient::new(self.channel.clone());
        let response = client
            .delete(proto_request)
            .await
            .map_err(|s| map_status(&self.socket, s))?
            .into_inner();
        let outcome = response
            .outcome
            .ok_or_else(|| ClientError::InvalidResponse("delete returned no outcome".into()))?;
        write_outcome_from_proto(&outcome).map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }
}

/// Register a new issuer. Build with [`RegisterIssuerRequest::new`], then
/// optionally mark [`dry_run`](Self::dry_run).
#[derive(Debug, Clone)]
pub struct RegisterIssuerRequest {
    issuer: Issuer,
    dry_run: bool,
}

impl RegisterIssuerRequest {
    pub fn new(issuer: Issuer) -> Self {
        Self {
            issuer,
            dry_run: false,
        }
    }

    /// Validate against the engine without persisting anything.
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

/// Patch an existing issuer under optimistic concurrency control. Build
/// with [`PatchIssuerRequest::new`], then optionally mark
/// [`dry_run`](Self::dry_run).
#[derive(Debug, Clone)]
pub struct PatchIssuerRequest {
    expected_version: u32,
    patch: IssuerPatch,
    dry_run: bool,
}

impl PatchIssuerRequest {
    pub fn new(expected_version: u32, patch: IssuerPatch) -> Self {
        Self {
            expected_version,
            patch,
            dry_run: false,
        }
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

/// Delete an existing issuer under optimistic concurrency control. Build
/// with [`DeleteIssuerRequest::new`], then optionally mark
/// [`dry_run`](Self::dry_run).
#[derive(Debug, Clone, Copy)]
pub struct DeleteIssuerRequest {
    expected_version: u32,
    dry_run: bool,
}

impl DeleteIssuerRequest {
    pub fn new(expected_version: u32) -> Self {
        Self {
            expected_version,
            dry_run: false,
        }
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IssuerMappingError {
    #[error("invalid issuer id: {0}")]
    InvalidId(String),

    #[error("invalid created_at timestamp: {0}")]
    InvalidTimestamp(String),

    #[error(transparent)]
    Name(#[from] IssuerNameError),

    #[error(transparent)]
    Status(#[from] IssuerStatusError),

    #[error(transparent)]
    Cnpj(#[from] CnpjError),

    #[error(transparent)]
    Lei(#[from] LeiError),

    #[error(transparent)]
    CountryCode(#[from] CountryCodeError),

    #[error(transparent)]
    Builder(#[from] IssuerBuilderError),

    #[error("a patch must set at least one field")]
    EmptyPatch,

    #[error("missing required field `{0}`")]
    MissingField(&'static str),
}

fn write_outcome_from_proto(msg: &WriteOutcomeProto) -> Result<WriteOutcome, IssuerMappingError> {
    match msg.outcome.as_ref() {
        Some(Outcome::Applied(_)) => Ok(WriteOutcome::Applied),
        Some(Outcome::VersionMismatch(vm)) => Ok(WriteOutcome::VersionMismatch {
            expected: vm.expected,
            actual: vm.actual,
        }),
        Some(Outcome::Missing(_)) => Ok(WriteOutcome::Missing),
        None => Err(IssuerMappingError::MissingField("outcome")),
    }
}

fn issuer_from_proto(msg: &IssuerProto) -> Result<Versioned<Issuer>, IssuerMappingError> {
    let mut builder = Issuer::builder()
        .id(parse_issuer_id(&msg.id)?)
        .status(IssuerStatus::from_str(&msg.status)?)
        .created_at(parse_timestamp(&msg.created_at)?);

    if let Some(name) = msg.name.as_deref() {
        builder = builder.name(IssuerName::new(name)?);
    }
    if let Some(cnpj) = msg.cnpj.as_deref() {
        builder = builder.cnpj(Cnpj::parse(cnpj)?);
    }
    if let Some(lei) = msg.lei.as_deref() {
        builder = builder.lei(Lei::parse(lei)?);
    }
    if let Some(cc) = msg.country_code.as_deref() {
        builder = builder.country_code(CountryCode::parse(cc)?);
    }

    Ok(Versioned {
        data: builder.build()?,
        version: msg.version,
    })
}

pub fn parse_issuer_id(raw: &str) -> Result<IssuerId, IssuerMappingError> {
    Uuid::from_str(raw)
        .map(IssuerId::from_uuid)
        .map_err(|e| IssuerMappingError::InvalidId(e.to_string()))
}

fn parse_timestamp(raw: &str) -> Result<DateTime<Utc>, IssuerMappingError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| IssuerMappingError::InvalidTimestamp(e.to_string()))
}

fn issuer_to_register_request(issuer: &Issuer, dry_run: bool) -> RegisterIssuerRequestProto {
    RegisterIssuerRequestProto {
        name: issuer.name().map(|n| n.as_str().to_string()),
        status: Some(String::from(issuer.status())),
        cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
        lei: issuer.lei().map(|l| l.as_str().to_string()),
        country_code: issuer.country_code().map(|c| c.as_str().to_string()),
        dry_run,
    }
}

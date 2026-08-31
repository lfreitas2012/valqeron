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

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn dry_run_if(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

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

fn parse_issuer_id(raw: &str) -> Result<IssuerId, IssuerMappingError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tonic::{Request, Response, Status};
    use valqeron_engine_proto::v1::rpc_issuer_service_server::{
        RpcIssuerService, RpcIssuerServiceServer,
    };
    use valqeron_engine_proto::v1::write_outcome_proto::{Applied, Missing, VersionMismatch};
    use valqeron_engine_proto::v1::{
        DeleteIssuerResponseProto, GetIssuerResponseProto, ListIssuersResponseProto,
        PatchIssuerResponseProto, RegisterIssuerResponseProto,
    };

    fn sample_issuer() -> Issuer {
        Issuer::builder()
            .id(IssuerId::from_uuid(
                Uuid::parse_str("018f6e80-8e2b-7b00-a54b-d7d8e9f01234").unwrap(),
            ))
            .status(IssuerStatus::Active)
            .created_at(
                DateTime::parse_from_rfc3339("2026-08-31T20:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .name(IssuerName::new("Valqeron Corp").unwrap())
            .cnpj(Cnpj::parse("11222333000181").unwrap())
            .lei(Lei::parse("5493006MHB84DD0ZWV18").unwrap())
            .country_code(CountryCode::parse("BR").unwrap())
            .build()
            .unwrap()
    }

    fn sample_issuer_proto() -> IssuerProto {
        IssuerProto {
            id: "018f6e80-8e2b-7b00-a54b-d7d8e9f01234".to_string(),
            version: 1,
            name: Some("Valqeron Corp".to_string()),
            status: "ACTIVE".to_string(),
            cnpj: Some("11222333000181".to_string()),
            lei: Some("5493006MHB84DD0ZWV18".to_string()),
            country_code: Some("BR".to_string()),
            created_at: "2026-08-31T20:00:00Z".to_string(),
        }
    }

    #[derive(Default)]
    struct MockIssuerServer {
        register_response: Mutex<Option<Result<RegisterIssuerResponseProto, Status>>>,
        get_response: Mutex<Option<Result<GetIssuerResponseProto, Status>>>,
        list_response: Mutex<Option<Result<ListIssuersResponseProto, Status>>>,
        patch_response: Mutex<Option<Result<PatchIssuerResponseProto, Status>>>,
        delete_response: Mutex<Option<Result<DeleteIssuerResponseProto, Status>>>,
        last_register_request: Mutex<Option<RegisterIssuerRequestProto>>,
        last_get_request: Mutex<Option<GetIssuerRequestProto>>,
        last_list_request: Mutex<Option<ListIssuersRequestProto>>,
        last_patch_request: Mutex<Option<PatchIssuerRequestProto>>,
        last_delete_request: Mutex<Option<DeleteIssuerRequestProto>>,
    }

    #[tonic::async_trait]
    impl RpcIssuerService for MockIssuerServer {
        async fn register(
            &self,
            request: Request<RegisterIssuerRequestProto>,
        ) -> Result<Response<RegisterIssuerResponseProto>, Status> {
            *self.last_register_request.lock().await = Some(request.into_inner());
            self.register_response
                .lock()
                .await
                .take()
                .unwrap_or_else(|| Ok(RegisterIssuerResponseProto::default()))
                .map(Response::new)
        }

        async fn get(
            &self,
            request: Request<GetIssuerRequestProto>,
        ) -> Result<Response<GetIssuerResponseProto>, Status> {
            *self.last_get_request.lock().await = Some(request.into_inner());
            self.get_response
                .lock()
                .await
                .take()
                .unwrap_or_else(|| Ok(GetIssuerResponseProto::default()))
                .map(Response::new)
        }

        async fn list(
            &self,
            request: Request<ListIssuersRequestProto>,
        ) -> Result<Response<ListIssuersResponseProto>, Status> {
            *self.last_list_request.lock().await = Some(request.into_inner());
            self.list_response
                .lock()
                .await
                .take()
                .unwrap_or_else(|| Ok(ListIssuersResponseProto::default()))
                .map(Response::new)
        }

        async fn patch(
            &self,
            request: Request<PatchIssuerRequestProto>,
        ) -> Result<Response<PatchIssuerResponseProto>, Status> {
            *self.last_patch_request.lock().await = Some(request.into_inner());
            self.patch_response
                .lock()
                .await
                .take()
                .unwrap_or_else(|| Ok(PatchIssuerResponseProto::default()))
                .map(Response::new)
        }

        async fn delete(
            &self,
            request: Request<DeleteIssuerRequestProto>,
        ) -> Result<Response<DeleteIssuerResponseProto>, Status> {
            *self.last_delete_request.lock().await = Some(request.into_inner());
            self.delete_response
                .lock()
                .await
                .take()
                .unwrap_or_else(|| Ok(DeleteIssuerResponseProto::default()))
                .map(Response::new)
        }
    }

    async fn spawn_mock_server(
        mock: Arc<MockIssuerServer>,
    ) -> (IssuerService, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let server_mock = mock.clone();
        let server_handle = tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(RpcIssuerServiceServer::from_arc(server_mock))
                .serve(addr)
                .await;
        });

        let endpoint =
            tonic::transport::Endpoint::from_shared(format!("http://127.0.0.1:{}", addr.port()))
                .unwrap();

        // Retry connecting until the test server is ready
        let mut channel = None;
        for _ in 0..50 {
            if let Ok(ch) = endpoint.connect().await {
                channel = Some(ch);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let channel = channel.expect("failed to connect to mock test server");
        let service = IssuerService::new(channel, PathBuf::from("/tmp/mock_valqeron.sock"));
        (service, server_handle)
    }

    #[test]
    fn register_request_builder() {
        let issuer = sample_issuer();
        let req = RegisterIssuerRequest::new(issuer.clone());
        assert!(!req.dry_run);

        let req = req.dry_run();
        assert!(req.dry_run);

        let req = req.dry_run_if(false);
        assert!(!req.dry_run);

        let req = req.dry_run_if(true);
        assert!(req.dry_run);

        let cloned = req.clone();
        assert!(cloned.dry_run);
    }

    #[test]
    fn patch_request_builder() {
        let patch = IssuerPatch::builder().status(IssuerStatus::Retired).build();
        let req = PatchIssuerRequest::new(5, patch);
        assert_eq!(req.expected_version, 5);
        assert!(!req.dry_run);

        let req = req.dry_run();
        assert!(req.dry_run);

        let cloned = req.clone();
        assert_eq!(cloned.expected_version, 5);
        assert!(cloned.dry_run);
    }

    #[test]
    fn delete_request_builder() {
        let req = DeleteIssuerRequest::new(3);
        assert_eq!(req.expected_version, 3);
        assert!(!req.dry_run);

        let req = req.dry_run();
        assert!(req.dry_run);

        let copied = req;
        assert_eq!(copied.expected_version, 3);
        assert!(copied.dry_run);
    }

    #[test]
    fn write_outcome_from_proto_mapping() {
        let applied = WriteOutcomeProto {
            outcome: Some(Outcome::Applied(Applied {})),
        };
        assert_eq!(
            write_outcome_from_proto(&applied).unwrap(),
            WriteOutcome::Applied
        );

        let vm = WriteOutcomeProto {
            outcome: Some(Outcome::VersionMismatch(VersionMismatch {
                expected: 2,
                actual: 4,
            })),
        };
        assert_eq!(
            write_outcome_from_proto(&vm).unwrap(),
            WriteOutcome::VersionMismatch {
                expected: 2,
                actual: 4,
            }
        );

        let missing = WriteOutcomeProto {
            outcome: Some(Outcome::Missing(Missing {})),
        };
        assert_eq!(
            write_outcome_from_proto(&missing).unwrap(),
            WriteOutcome::Missing
        );

        let empty = WriteOutcomeProto { outcome: None };
        assert!(matches!(
            write_outcome_from_proto(&empty).unwrap_err(),
            IssuerMappingError::MissingField("outcome")
        ));
    }

    #[test]
    fn issuer_from_proto_full_and_minimal() {
        let proto = sample_issuer_proto();
        let versioned = issuer_from_proto(&proto).unwrap();
        assert_eq!(versioned.version, 1);
        assert_eq!(
            versioned.data.id().value(),
            "018f6e80-8e2b-7b00-a54b-d7d8e9f01234"
        );
        assert_eq!(versioned.data.status(), IssuerStatus::Active);
        assert_eq!(
            versioned.data.name().map(IssuerName::as_str),
            Some("Valqeron Corp")
        );
        assert_eq!(
            versioned.data.cnpj().map(Cnpj::as_str),
            Some("11222333000181")
        );
        assert_eq!(
            versioned.data.lei().map(Lei::as_str),
            Some("5493006MHB84DD0ZWV18")
        );
        assert_eq!(
            versioned.data.country_code().map(CountryCode::as_str),
            Some("BR")
        );

        let minimal_proto = IssuerProto {
            id: "018f6e80-8e2b-7b00-a54b-d7d8e9f01234".to_string(),
            version: 2,
            name: None,
            status: "RETIRED".to_string(),
            cnpj: None,
            lei: None,
            country_code: None,
            created_at: "2026-08-31T20:00:00Z".to_string(),
        };
        let min_versioned = issuer_from_proto(&minimal_proto).unwrap();
        assert_eq!(min_versioned.version, 2);
        assert_eq!(min_versioned.data.status(), IssuerStatus::Retired);
        assert!(min_versioned.data.name().is_none());
        assert!(min_versioned.data.cnpj().is_none());
        assert!(min_versioned.data.lei().is_none());
        assert!(min_versioned.data.country_code().is_none());
    }

    #[test]
    fn issuer_from_proto_invalid_fields() {
        let valid = sample_issuer_proto();

        let mut invalid_id = valid.clone();
        invalid_id.id = "not-a-uuid".to_string();
        assert!(matches!(
            issuer_from_proto(&invalid_id).unwrap_err(),
            IssuerMappingError::InvalidId(_)
        ));

        let mut invalid_ts = valid.clone();
        invalid_ts.created_at = "not-a-timestamp".to_string();
        assert!(matches!(
            issuer_from_proto(&invalid_ts).unwrap_err(),
            IssuerMappingError::InvalidTimestamp(_)
        ));

        let mut invalid_status = valid.clone();
        invalid_status.status = "UNKNOWN_STATUS".to_string();
        assert!(matches!(
            issuer_from_proto(&invalid_status).unwrap_err(),
            IssuerMappingError::Status(_)
        ));

        let mut invalid_cnpj = valid.clone();
        invalid_cnpj.cnpj = Some("123".to_string());
        assert!(matches!(
            issuer_from_proto(&invalid_cnpj).unwrap_err(),
            IssuerMappingError::Cnpj(_)
        ));

        let mut invalid_lei = valid.clone();
        invalid_lei.lei = Some("invalid-lei".to_string());
        assert!(matches!(
            issuer_from_proto(&invalid_lei).unwrap_err(),
            IssuerMappingError::Lei(_)
        ));

        let mut invalid_cc = valid.clone();
        invalid_cc.country_code = Some("INVALID".to_string());
        assert!(matches!(
            issuer_from_proto(&invalid_cc).unwrap_err(),
            IssuerMappingError::CountryCode(_)
        ));

        let mut invalid_name = valid.clone();
        invalid_name.name = Some("".to_string());
        assert!(matches!(
            issuer_from_proto(&invalid_name).unwrap_err(),
            IssuerMappingError::Name(_)
        ));
    }

    #[test]
    fn parse_issuer_id_and_timestamp() {
        let valid_uuid = "018f6e80-8e2b-7b00-a54b-d7d8e9f01234";
        assert_eq!(
            parse_issuer_id(valid_uuid).unwrap().value(),
            valid_uuid
        );
        assert!(parse_issuer_id("invalid").is_err());

        let rfc3339_z = "2026-08-31T20:00:00Z";
        assert_eq!(
            parse_timestamp(rfc3339_z).unwrap().to_rfc3339(),
            "2026-08-31T20:00:00+00:00"
        );

        let rfc3339_offset = "2026-08-31T22:00:00+02:00";
        assert_eq!(
            parse_timestamp(rfc3339_offset).unwrap().to_rfc3339(),
            "2026-08-31T20:00:00+00:00"
        );

        assert!(parse_timestamp("bad-time").is_err());
    }

    #[test]
    fn issuer_to_register_request_mapping() {
        let issuer = sample_issuer();
        let proto = issuer_to_register_request(&issuer, true);
        assert!(proto.dry_run);
        assert_eq!(proto.name.as_deref(), Some("Valqeron Corp"));
        assert_eq!(proto.status.as_deref(), Some("ACTIVE"));
        assert_eq!(proto.cnpj.as_deref(), Some("11222333000181"));
        assert_eq!(proto.lei.as_deref(), Some("5493006MHB84DD0ZWV18"));
        assert_eq!(proto.country_code.as_deref(), Some("BR"));

        let proto_false = issuer_to_register_request(&issuer, false);
        assert!(!proto_false.dry_run);
    }

    #[test]
    fn error_display_messages() {
        let err_id = IssuerMappingError::InvalidId("abc".to_string());
        assert!(err_id.to_string().contains("invalid issuer id: abc"));

        let err_ts = IssuerMappingError::InvalidTimestamp("xyz".to_string());
        assert!(err_ts.to_string().contains("invalid created_at timestamp: xyz"));

        let err_field = IssuerMappingError::MissingField("outcome");
        assert_eq!(
            err_field.to_string(),
            "missing required field `outcome`"
        );

        let err_name: IssuerMappingError = IssuerName::new("").unwrap_err().into();
        assert!(err_name.to_string().contains("empty"));

        let err_status: IssuerMappingError = IssuerStatus::from_str("INVALID").unwrap_err().into();
        assert!(err_status.to_string().contains("Invalid status"));

        let err_cnpj: IssuerMappingError = Cnpj::parse("123").unwrap_err().into();
        assert!(!err_cnpj.to_string().is_empty());

        let err_lei: IssuerMappingError = Lei::parse("bad").unwrap_err().into();
        assert!(!err_lei.to_string().is_empty());

        let err_cc: IssuerMappingError = CountryCode::parse("XYZ").unwrap_err().into();
        assert!(!err_cc.to_string().is_empty());
    }

    #[tokio::test]
    async fn service_register_success_and_errors() {
        let mock = Arc::new(MockIssuerServer::default());
        let (service, handle) = spawn_mock_server(mock.clone()).await;

        // Success
        *mock.register_response.lock().await = Some(Ok(RegisterIssuerResponseProto {
            issuer: Some(sample_issuer_proto()),
        }));
        let registered = service
            .register(RegisterIssuerRequest::new(sample_issuer()).dry_run())
            .await
            .unwrap();
        assert_eq!(registered.version, 1);
        assert_eq!(
            mock.last_register_request.lock().await.as_ref().unwrap().dry_run,
            true
        );

        // Missing issuer in response
        *mock.register_response.lock().await = Some(Ok(RegisterIssuerResponseProto {
            issuer: None,
        }));
        let err = service
            .register(RegisterIssuerRequest::new(sample_issuer()))
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::InvalidResponse(msg) if msg.contains("register returned no issuer")));

        // Invalid issuer proto in response
        let mut invalid_proto = sample_issuer_proto();
        invalid_proto.id = "bad-uuid".to_string();
        *mock.register_response.lock().await = Some(Ok(RegisterIssuerResponseProto {
            issuer: Some(invalid_proto),
        }));
        let err = service
            .register(RegisterIssuerRequest::new(sample_issuer()))
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::InvalidResponse(_)));

        // RPC status error
        *mock.register_response.lock().await = Some(Err(Status::already_exists("already exists")));
        let err = service
            .register(RegisterIssuerRequest::new(sample_issuer()))
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Rpc { .. }));

        // Unavailable error maps to Unreachable
        *mock.register_response.lock().await = Some(Err(Status::unavailable("server unavailable")));
        let err = service
            .register(RegisterIssuerRequest::new(sample_issuer()))
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Unreachable { .. }));

        handle.abort();
    }

    #[tokio::test]
    async fn service_get_found_none_and_errors() {
        let mock = Arc::new(MockIssuerServer::default());
        let (service, handle) = spawn_mock_server(mock.clone()).await;
        let id = sample_issuer().id().clone();

        // Found
        *mock.get_response.lock().await = Some(Ok(GetIssuerResponseProto {
            issuer: Some(sample_issuer_proto()),
        }));
        let res = service.get(&id).await.unwrap();
        assert!(res.is_some());
        assert_eq!(res.unwrap().data.id(), &id);
        assert_eq!(
            mock.last_get_request.lock().await.as_ref().unwrap().id,
            id.value()
        );

        // Not found
        *mock.get_response.lock().await = Some(Ok(GetIssuerResponseProto { issuer: None }));
        let res = service.get(&id).await.unwrap();
        assert!(res.is_none());

        // Invalid proto
        let mut invalid_proto = sample_issuer_proto();
        invalid_proto.status = "BAD_STATUS".to_string();
        *mock.get_response.lock().await = Some(Ok(GetIssuerResponseProto {
            issuer: Some(invalid_proto),
        }));
        assert!(service.get(&id).await.is_err());

        // RPC error
        *mock.get_response.lock().await = Some(Err(Status::internal("database error")));
        let err = service.get(&id).await.unwrap_err();
        assert!(matches!(err, ClientError::Rpc { .. }));

        handle.abort();
    }

    #[tokio::test]
    async fn service_list_success_empty_and_errors() {
        let mock = Arc::new(MockIssuerServer::default());
        let (service, handle) = spawn_mock_server(mock.clone()).await;

        // Non-empty list
        *mock.list_response.lock().await = Some(Ok(ListIssuersResponseProto {
            issuers: vec![sample_issuer_proto()],
        }));
        let after_id = IssuerId::from_uuid(Uuid::now_v7());
        let list = service.list(Some(&after_id), 20).await.unwrap();
        assert_eq!(list.len(), 1);
        let req = mock.last_list_request.lock().await.clone().unwrap();
        assert_eq!(req.after.as_deref(), Some(after_id.value().as_str()));
        assert_eq!(req.limit, 20);

        // Empty list without after
        *mock.list_response.lock().await = Some(Ok(ListIssuersResponseProto {
            issuers: vec![],
        }));
        let list = service.list(None, 50).await.unwrap();
        assert!(list.is_empty());
        let req = mock.last_list_request.lock().await.clone().unwrap();
        assert!(req.after.is_none());
        assert_eq!(req.limit, 50);

        // Invalid item in list
        let mut invalid_proto = sample_issuer_proto();
        invalid_proto.created_at = "bad_ts".to_string();
        *mock.list_response.lock().await = Some(Ok(ListIssuersResponseProto {
            issuers: vec![invalid_proto],
        }));
        assert!(service.list(None, 10).await.is_err());

        // RPC error
        *mock.list_response.lock().await = Some(Err(Status::invalid_argument("invalid limit")));
        let err = service.list(None, 0).await.unwrap_err();
        assert!(matches!(err, ClientError::Rpc { .. }));

        handle.abort();
    }

    #[tokio::test]
    async fn service_patch_success_and_errors() {
        let mock = Arc::new(MockIssuerServer::default());
        let (service, handle) = spawn_mock_server(mock.clone()).await;
        let id = sample_issuer().id().clone();
        let patch = IssuerPatch::builder()
            .name(IssuerName::new("New Name").unwrap())
            .build();
        let patch_req = PatchIssuerRequest::new(1, patch).dry_run();

        // Applied
        *mock.patch_response.lock().await = Some(Ok(PatchIssuerResponseProto {
            outcome: Some(WriteOutcomeProto {
                outcome: Some(Outcome::Applied(Applied {})),
            }),
        }));
        let outcome = service.patch(&id, patch_req.clone()).await.unwrap();
        assert_eq!(outcome, WriteOutcome::Applied);
        let req = mock.last_patch_request.lock().await.clone().unwrap();
        assert_eq!(req.id, id.value());
        assert_eq!(req.expected_version, 1);
        assert_eq!(req.name.as_deref(), Some("New Name"));
        assert!(req.dry_run);

        // VersionMismatch
        *mock.patch_response.lock().await = Some(Ok(PatchIssuerResponseProto {
            outcome: Some(WriteOutcomeProto {
                outcome: Some(Outcome::VersionMismatch(VersionMismatch {
                    expected: 1,
                    actual: 2,
                })),
            }),
        }));
        let outcome = service.patch(&id, patch_req.clone()).await.unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::VersionMismatch {
                expected: 1,
                actual: 2
            }
        );

        // Missing outcome
        *mock.patch_response.lock().await = Some(Ok(PatchIssuerResponseProto {
            outcome: None,
        }));
        let err = service.patch(&id, patch_req.clone()).await.unwrap_err();
        assert!(matches!(err, ClientError::InvalidResponse(msg) if msg.contains("patch returned no outcome")));

        // RPC error
        *mock.patch_response.lock().await = Some(Err(Status::not_found("issuer not found")));
        let err = service.patch(&id, patch_req).await.unwrap_err();
        assert!(matches!(err, ClientError::Rpc { .. }));

        handle.abort();
    }

    #[tokio::test]
    async fn service_delete_success_and_errors() {
        let mock = Arc::new(MockIssuerServer::default());
        let (service, handle) = spawn_mock_server(mock.clone()).await;
        let id = sample_issuer().id().clone();
        let del_req = DeleteIssuerRequest::new(4).dry_run();

        // Applied
        *mock.delete_response.lock().await = Some(Ok(DeleteIssuerResponseProto {
            outcome: Some(WriteOutcomeProto {
                outcome: Some(Outcome::Applied(Applied {})),
            }),
        }));
        let outcome = service.delete(&id, del_req).await.unwrap();
        assert_eq!(outcome, WriteOutcome::Applied);
        let req = mock.last_delete_request.lock().await.clone().unwrap();
        assert_eq!(req.id, id.value());
        assert_eq!(req.expected_version, 4);
        assert!(req.dry_run);

        // Missing outcome
        *mock.delete_response.lock().await = Some(Ok(DeleteIssuerResponseProto {
            outcome: None,
        }));
        let err = service.delete(&id, DeleteIssuerRequest::new(1)).await.unwrap_err();
        assert!(matches!(err, ClientError::InvalidResponse(msg) if msg.contains("delete returned no outcome")));

        // RPC error
        *mock.delete_response.lock().await = Some(Err(Status::failed_precondition("cannot delete")));
        let err = service.delete(&id, DeleteIssuerRequest::new(1)).await.unwrap_err();
        assert!(matches!(err, ClientError::Rpc { .. }));

        handle.abort();
    }
}

//! Maps every failure a gRPC handler can produce to exactly one `(gRPC code, RFC-7807 problem)` pair.

use serde_json::{Map, Value};
use tonic::{Code, Status};
use valqeron_common::{collect_problem_detail_cause, extensions_json};
use valqeron_core::{
    CnpjError, IssuerBuilderError, IssuerNameError, RegisterIssuerError, StorageError, StorageFault,
};
use valqeron_proto::{IssuerMappingError, status_with_problem, v1};

use crate::storage::StorageCallError;

/// Sysexits-style statuses shared with the CLI's problem taxonomy.
mod exit {
    /// A requested entity was not found.
    pub const NOTFOUND: u32 = 4;
    /// A uniqueness / optimistic-lock conflict.
    pub const CONFLICT: u32 = 9;
    /// The input data was incorrect in some way (validation).
    pub const DATAERR: u32 = 65;
    /// An input payload could not be read / parsed.
    pub const NOINPUT: u32 = 66;
    /// The engine cannot accept work right now (shutting down).
    pub const UNAVAILABLE: u32 = 69;
    /// An internal software error.
    pub const SOFTWARE: u32 = 70;
    /// A transient overload; the caller should retry later.
    pub const TEMPFAIL: u32 = 75;
    /// Storage engine error.
    pub const STORAGE: u32 = 80;
}

/// Everything an engine gRPC handler can fail with.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error(transparent)]
    Mapping(#[from] IssuerMappingError),

    #[error(transparent)]
    Register(#[from] RegisterIssuerError),

    #[error(transparent)]
    Fault(#[from] StorageFault),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Call(#[from] StorageCallError),
}

impl HandlerError {
    /// Convert into a `tonic::Status` carrying the RFC-7807 document in its
    /// details.
    pub fn into_status(self) -> Status {
        let code = self.code();
        let problem = v1::ProblemDetailProto {
            r#type: self.problem_type().to_string(),
            title: self.title().to_string(),
            status: self.status(),
            detail: self.to_string(),
            extensions_json: extensions_json(&self.extensions()),
            causes: collect_problem_detail_cause(&self),
        };
        status_with_problem(code, problem)
    }

    fn code(&self) -> Code {
        match self {
            HandlerError::Mapping(_) => Code::InvalidArgument,
            HandlerError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj | RegisterIssuerError::DuplicateLei => {
                    Code::AlreadyExists
                }
                RegisterIssuerError::Storage(_) => Code::Internal,
            },
            HandlerError::Fault(_) | HandlerError::Storage(_) => Code::Internal,
            HandlerError::Call(e) => match e {
                StorageCallError::Overloaded => Code::ResourceExhausted,
                StorageCallError::ShuttingDown => Code::Unavailable,
                StorageCallError::TaskFailed(_) => Code::Internal,
            },
        }
    }

    fn problem_type(&self) -> &'static str {
        match self {
            HandlerError::Mapping(e) => match e {
                IssuerMappingError::InvalidId(_) => "issuer/invalid-id",
                IssuerMappingError::InvalidTimestamp(_) => "issuer/validation/timestamp",
                IssuerMappingError::Name(IssuerNameError::Empty) => "issuer/validation/name-empty",
                IssuerMappingError::Name(IssuerNameError::TooLong { .. }) => {
                    "issuer/validation/name-too-long"
                }
                IssuerMappingError::Status(_) => "issuer/validation/status",
                IssuerMappingError::Cnpj(_) => "identifier/cnpj-invalid",
                IssuerMappingError::Lei(_) => "identifier/lei-invalid",
                IssuerMappingError::CountryCode(_) => "identifier/country-code-invalid",
                IssuerMappingError::Builder(b) => match b {
                    IssuerBuilderError::InvalidCountryForCnpj(_) => {
                        "issuer/validation/country-cnpj-mismatch"
                    }
                    IssuerBuilderError::NameError(_) => "issuer/validation/name",
                    IssuerBuilderError::CountryCodeError(_) => "identifier/country-code-invalid",
                },
                IssuerMappingError::EmptyPatch => "issuer/validation/empty-patch",
                IssuerMappingError::MissingField(_) => "input/parse",
            },
            HandlerError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj => "issuer/duplicate-cnpj",
                RegisterIssuerError::DuplicateLei => "issuer/duplicate-lei",
                RegisterIssuerError::Storage(_) => "storage/failed",
            },
            HandlerError::Fault(_) | HandlerError::Storage(_) => "storage/failed",
            HandlerError::Call(e) => match e {
                StorageCallError::Overloaded => "engine/overloaded",
                StorageCallError::ShuttingDown => "engine/unavailable",
                StorageCallError::TaskFailed(_) => "engine/internal",
            },
        }
    }

    fn title(&self) -> &'static str {
        match self {
            HandlerError::Mapping(e) => match e {
                IssuerMappingError::InvalidId(_) => "Invalid issuer id",
                IssuerMappingError::Cnpj(_)
                | IssuerMappingError::Lei(_)
                | IssuerMappingError::CountryCode(_) => "Invalid identifier",
                IssuerMappingError::MissingField(_) => "Invalid input",
                _ => "Issuer validation failed",
            },
            HandlerError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj | RegisterIssuerError::DuplicateLei => {
                    "Duplicate identifier"
                }
                RegisterIssuerError::Storage(_) => "Storage error",
            },
            HandlerError::Fault(_) | HandlerError::Storage(_) => "Storage error",
            HandlerError::Call(e) => match e {
                StorageCallError::Overloaded => "Engine overloaded",
                StorageCallError::ShuttingDown => "Engine unavailable",
                StorageCallError::TaskFailed(_) => "Engine internal error",
            },
        }
    }

    fn status(&self) -> u32 {
        match self {
            HandlerError::Mapping(e) => match e {
                IssuerMappingError::MissingField(_) => exit::NOINPUT,
                _ => exit::DATAERR,
            },
            HandlerError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj | RegisterIssuerError::DuplicateLei => {
                    exit::CONFLICT
                }
                RegisterIssuerError::Storage(_) => exit::STORAGE,
            },
            HandlerError::Fault(_) | HandlerError::Storage(_) => exit::STORAGE,
            HandlerError::Call(e) => match e {
                StorageCallError::Overloaded => exit::TEMPFAIL,
                StorageCallError::ShuttingDown => exit::UNAVAILABLE,
                StorageCallError::TaskFailed(_) => exit::SOFTWARE,
            },
        }
    }

    fn extensions(&self) -> Map<String, Value> {
        let mut ext = Map::new();
        match self {
            HandlerError::Mapping(e) => match e {
                IssuerMappingError::Name(IssuerNameError::TooLong { max }) => {
                    ext.insert("max".into(), Value::from(*max));
                }
                IssuerMappingError::Status(_) => {
                    ext.insert("allowed".into(), Value::from(vec!["ACTIVE", "RETIRED"]));
                }
                IssuerMappingError::Cnpj(err) => {
                    ext.insert("field".into(), Value::from("cnpj"));
                    if let CnpjError::InvalidCheckDigits {
                        position,
                        expected,
                        found,
                    } = err
                    {
                        ext.insert("position".into(), Value::from(*position));
                        ext.insert("expected".into(), Value::from(*expected));
                        ext.insert("found".into(), Value::from(*found));
                    }
                }
                IssuerMappingError::Lei(_) => {
                    ext.insert("field".into(), Value::from("lei"));
                }
                IssuerMappingError::CountryCode(_) => {
                    ext.insert("field".into(), Value::from("country_code"));
                }
                IssuerMappingError::Builder(IssuerBuilderError::InvalidCountryForCnpj(cc)) => {
                    ext.insert("country_code".into(), Value::from(cc.clone()));
                }
                _ => {}
            },
            HandlerError::Register(RegisterIssuerError::DuplicateCnpj) => {
                ext.insert("field".into(), Value::from("cnpj"));
            }
            HandlerError::Register(RegisterIssuerError::DuplicateLei) => {
                ext.insert("field".into(), Value::from("lei"));
            }
            _ => {}
        }
        ext
    }
}

/// The problem document for an issuer that does not exist (used where a
/// handler treats absence as an error rather than an empty result).
#[allow(dead_code)]
pub fn not_found_problem(id: &str) -> Status {
    let mut ext = Map::new();
    ext.insert("id".into(), Value::from(id));
    let problem = v1::ProblemDetailProto {
        r#type: "issuer/not-found".to_string(),
        title: "Issuer not found".to_string(),
        status: exit::NOTFOUND,
        detail: format!("issuer {id} not found"),
        extensions_json: extensions_json(&ext),
        causes: Vec::new(),
    };
    status_with_problem(Code::NotFound, problem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use valqeron_proto::problem_from_status;

    #[test]
    fn duplicate_cnpj_keeps_the_cli_contract() {
        let status = HandlerError::Register(RegisterIssuerError::DuplicateCnpj).into_status();
        assert_eq!(status.code(), Code::AlreadyExists);
        let problem = problem_from_status(&status).expect("problem attached");
        assert_eq!(problem.r#type, "issuer/duplicate-cnpj");
        assert_eq!(problem.status, exit::CONFLICT);
        assert_eq!(problem.title, "Duplicate identifier");
        assert!(problem.extensions_json.contains("\"field\":\"cnpj\""));
    }

    #[test]
    fn bad_cnpj_surfaces_check_digit_extensions() {
        let err = valqeron_core::Cnpj::parse("00000000000192").expect_err("invalid check digit");
        let status = HandlerError::Mapping(IssuerMappingError::Cnpj(err)).into_status();
        assert_eq!(status.code(), Code::InvalidArgument);
        let problem = problem_from_status(&status).expect("problem attached");
        assert_eq!(problem.r#type, "identifier/cnpj-invalid");
        assert_eq!(problem.status, exit::DATAERR);
        for key in ["field", "position", "expected", "found"] {
            assert!(
                problem.extensions_json.contains(key),
                "missing extension {key}: {}",
                problem.extensions_json
            );
        }
    }

    #[test]
    fn backpressure_maps_to_resource_exhausted() {
        let status = HandlerError::Call(StorageCallError::Overloaded).into_status();
        assert_eq!(status.code(), Code::ResourceExhausted);
        let problem = problem_from_status(&status).expect("problem attached");
        assert_eq!(problem.r#type, "engine/overloaded");
        assert_eq!(problem.status, exit::TEMPFAIL);
    }

    #[test]
    fn name_too_long_reports_max() {
        let err = IssuerMappingError::Name(IssuerNameError::TooLong { max: 200 });
        let status = HandlerError::Mapping(err).into_status();
        let problem = problem_from_status(&status).expect("problem attached");
        assert_eq!(problem.r#type, "issuer/validation/name-too-long");
        assert!(problem.extensions_json.contains("\"max\":200"));
    }
}

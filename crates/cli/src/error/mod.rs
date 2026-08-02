pub mod problem;

use ftracker_identifiers::{CnpjError, CountryCodeError, LeiError};
use problem::{IntoProblem, ProblemDetail};
use serde_json::{Map, Value};
use std::borrow::Cow;
use valqeron_core::{
    IssuerBuilderError, IssuerNameError, IssuerStatusError, RegisterIssuerError, StorageError,
    StorageFault,
};

pub type AppResult<T> = Result<T, AppError>;

mod exit {
    /// A requested entity was not found.
    pub const NOTFOUND: u16 = 4;
    /// A uniqueness / optimistic-lock conflict.
    pub const CONFLICT: u16 = 9;
    /// Wrong command line arguments.
    pub const USAGE: u16 = 64;
    /// The input data was incorrect in some way (validation).
    pub const DATAERR: u16 = 65;
    /// An input file could not be read / parsed.
    pub const NOINPUT: u16 = 66;
    /// A migration or internal software error.
    pub const SOFTWARE: u16 = 70;
    /// An I/O error occurred.
    pub const IOERR: u16 = 74;
    /// Something was misconfigured.
    pub const CONFIG: u16 = 78;
    /// Storage engine error.
    pub const STORAGE: u16 = 80;
}

#[derive(Debug, Clone, Copy)]
pub enum IdentifierKind {
    Cnpj,
    Lei,
    CountryCode,
}

impl IdentifierKind {
    fn slug(self) -> &'static str {
        match self {
            IdentifierKind::Cnpj => "cnpj",
            IdentifierKind::Lei => "lei",
            IdentifierKind::CountryCode => "country-code",
        }
    }

    fn field(self) -> &'static str {
        match self {
            IdentifierKind::Cnpj => "cnpj",
            IdentifierKind::Lei => "lei",
            IdentifierKind::CountryCode => "country_code",
        }
    }
}

impl std::fmt::Display for IdentifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Register(#[from] RegisterIssuerError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    StorageFault(#[from] StorageFault),

    #[error(transparent)]
    IssuerBuilder(#[from] IssuerBuilderError),

    #[error(transparent)]
    IssuerName(#[from] IssuerNameError),

    #[error(transparent)]
    IssuerStatus(#[from] IssuerStatusError),

    #[error("invalid {kind} identifier: {message}")]
    Identifier {
        kind: IdentifierKind,
        message: String,
        extensions: Map<String, Value>,
    },

    #[error("invalid issuer id: {0}")]
    InvalidId(String),

    #[error("failed to read JSON input: {0}")]
    Input(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("failed to serialize output: {0}")]
    Serialize(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[allow(dead_code)]
    #[error("issuer {0} not found")]
    NotFound(String),

    #[allow(dead_code)]
    #[error("version conflict: expected {expected}, found {actual}")]
    VersionConflict { expected: u32, actual: u32 },

    #[error("Command line flag `{flag}` is not valid for `{command}`.")]
    InvalidCliFlag { flag: String, command: String },
}

impl AppError {
    fn identifier(kind: IdentifierKind, err: &dyn std::error::Error) -> Self {
        let mut extensions = Map::new();
        extensions.insert("field".into(), Value::from(kind.field()));
        AppError::Identifier {
            kind,
            message: err.to_string(),
            extensions,
        }
    }

    pub fn problem(&self) -> ProblemDetail {
        self.to_problem_detail()
    }
}

impl From<CnpjError> for AppError {
    fn from(err: CnpjError) -> Self {
        let mut base = AppError::identifier(IdentifierKind::Cnpj, &err);
        if let AppError::Identifier { extensions, .. } = &mut base {
            // Surface the most useful structured detail: mismatching check digits.
            if let CnpjError::InvalidCheckDigits {
                position,
                expected,
                found,
            } = err
            {
                extensions.insert("position".into(), Value::from(position));
                extensions.insert("expected".into(), Value::from(expected));
                extensions.insert("found".into(), Value::from(found));
            }
        }
        base
    }
}

impl From<LeiError> for AppError {
    fn from(err: LeiError) -> Self {
        AppError::identifier(IdentifierKind::Lei, &err)
    }
}

impl From<CountryCodeError> for AppError {
    fn from(err: CountryCodeError) -> Self {
        AppError::identifier(IdentifierKind::CountryCode, &err)
    }
}

impl IntoProblem for AppError {
    fn problem_type(&self) -> &'static str {
        match self {
            AppError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj => "issuer/duplicate-cnpj",
                RegisterIssuerError::DuplicateLei => "issuer/duplicate-lei",
                RegisterIssuerError::Storage(_) => "storage/failed",
            },
            AppError::Storage(_) | AppError::StorageFault(_) => "storage/failed",
            AppError::IssuerBuilder(e) => match e {
                IssuerBuilderError::InvalidCountryForCnpj(_) => {
                    "issuer/validation/country-cnpj-mismatch"
                }
                IssuerBuilderError::NameError(_) => "issuer/validation/name",
            },
            AppError::IssuerName(e) => match e {
                IssuerNameError::Empty => "issuer/validation/name-empty",
                IssuerNameError::TooLong { .. } => "issuer/validation/name-too-long",
            },
            AppError::IssuerStatus(_) => "issuer/validation/status",
            AppError::Identifier { kind, .. } => match kind {
                IdentifierKind::Cnpj => "identifier/cnpj-invalid",
                IdentifierKind::Lei => "identifier/lei-invalid",
                IdentifierKind::CountryCode => "identifier/country-code-invalid",
            },
            AppError::InvalidId(_) => "issuer/invalid-id",
            AppError::NotFound(_) => "issuer/not-found",
            AppError::VersionConflict { .. } => "issuer/conflict",
            AppError::Input(_) => "input/parse",
            AppError::Io(_) => "io/failed",
            AppError::Serialize(_) => "io/serialize-failed",
            AppError::Config(_) => "config/invalid",
            AppError::InvalidCliFlag { .. } => "cli/invalid-flag",
        }
    }

    fn title(&self) -> Cow<'static, str> {
        match self {
            AppError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj | RegisterIssuerError::DuplicateLei => {
                    Cow::Borrowed("Duplicate identifier")
                }
                RegisterIssuerError::Storage(_) => Cow::Borrowed("Storage error"),
            },
            AppError::Storage(_) | AppError::StorageFault(_) => Cow::Borrowed("Storage error"),
            AppError::IssuerBuilder(_) | AppError::IssuerName(_) | AppError::IssuerStatus(_) => {
                Cow::Borrowed("Issuer validation failed")
            }
            AppError::Identifier { .. } => Cow::Borrowed("Invalid identifier"),
            AppError::InvalidId(_) => Cow::Borrowed("Invalid issuer id"),
            AppError::NotFound(_) => Cow::Borrowed("Issuer not found"),
            AppError::VersionConflict { .. } => Cow::Borrowed("Conflict"),
            AppError::Input(_) => Cow::Borrowed("Invalid input"),
            AppError::Io(_) => Cow::Borrowed("I/O error"),
            AppError::Serialize(_) => Cow::Borrowed("Serialization error"),
            AppError::Config(_) => Cow::Borrowed("Invalid configuration"),

            AppError::InvalidCliFlag { flag, command } => {
                Cow::Owned(format!("Invalid flag `{flag}` for `{command}`"))
            }
        }
    }

    fn status(&self) -> u16 {
        match self {
            AppError::Register(e) => match e {
                RegisterIssuerError::DuplicateCnpj | RegisterIssuerError::DuplicateLei => {
                    exit::CONFLICT
                }
                RegisterIssuerError::Storage(_) => exit::STORAGE,
            },
            AppError::Storage(_) | AppError::StorageFault(_) => exit::STORAGE,
            AppError::IssuerBuilder(_)
            | AppError::IssuerName(_)
            | AppError::IssuerStatus(_)
            | AppError::Identifier { .. }
            | AppError::InvalidId(_) => exit::DATAERR,
            AppError::NotFound(_) => exit::NOTFOUND,
            AppError::VersionConflict { .. } => exit::CONFLICT,
            AppError::Input(_) => exit::NOINPUT,
            AppError::Io(_) => exit::IOERR,
            AppError::Serialize(_) => exit::SOFTWARE,
            AppError::Config(_) => exit::CONFIG,
            AppError::InvalidCliFlag { .. } => exit::USAGE,
        }
    }

    fn extensions(&self) -> Map<String, Value> {
        let mut ext = Map::new();
        match self {
            AppError::Register(RegisterIssuerError::DuplicateCnpj) => {
                ext.insert("field".into(), Value::from("cnpj"));
            }
            AppError::Register(RegisterIssuerError::DuplicateLei) => {
                ext.insert("field".into(), Value::from("lei"));
            }
            AppError::IssuerBuilder(IssuerBuilderError::InvalidCountryForCnpj(cc)) => {
                ext.insert("country_code".into(), Value::from(cc.clone()));
            }
            AppError::IssuerName(IssuerNameError::TooLong { max }) => {
                ext.insert("max".into(), Value::from(*max));
            }
            AppError::IssuerStatus(_) => {
                ext.insert("allowed".into(), Value::from(vec!["ACTIVE", "RETIRED"]));
            }
            AppError::NotFound(id) => {
                ext.insert("id".into(), Value::from(id.clone()));
            }
            AppError::VersionConflict { expected, actual } => {
                ext.insert("expected".into(), Value::from(*expected));
                ext.insert("actual".into(), Value::from(*actual));
            }
            AppError::Identifier { extensions, .. } => return extensions.clone(),
            _ => {}
        }
        ext
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_cnpj_maps_to_conflict_exit_with_field() {
        let err = AppError::Register(RegisterIssuerError::DuplicateCnpj);
        let p = err.problem();
        assert_eq!(p.r#type, "issuer/duplicate-cnpj");
        assert_eq!(p.status, exit::CONFLICT);
        assert_eq!(p.extensions.get("field").unwrap(), "cnpj");
    }

    #[test]
    fn storage_error_maps_to_storage_exit() {
        let err = AppError::Storage(StorageError::Unavailable("disk gone".into()));
        let p = err.problem();
        assert_eq!(p.r#type, "storage/failed");
        assert_eq!(p.status, exit::STORAGE);
    }

    #[test]
    fn bad_cnpj_surfaces_check_digit_fields() {
        let err: AppError = valqeron_core::Cnpj::parse("00000000000192")
            .unwrap_err()
            .into();
        let p = err.problem();
        assert_eq!(p.r#type, "identifier/cnpj-invalid");
        assert_eq!(p.status, exit::DATAERR);
        assert_eq!(p.extensions.get("field").unwrap(), "cnpj");
        // The sample fails on the check digits, so structured fields are present.
        assert!(p.extensions.contains_key("position"));
        assert!(p.extensions.contains_key("expected"));
        assert!(p.extensions.contains_key("found"));
    }

    #[test]
    fn name_too_long_reports_max() {
        let err = AppError::IssuerName(IssuerNameError::TooLong { max: 200 });
        let p = err.problem();
        assert_eq!(p.r#type, "issuer/validation/name-too-long");
        assert_eq!(p.status, exit::DATAERR);
        assert_eq!(p.extensions.get("max").unwrap(), 200);
    }
}

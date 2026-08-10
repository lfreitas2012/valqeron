pub mod problem;

use problem::{IntoProblem, ProblemDetail};
use serde_json::{Map, Value};
use std::borrow::Cow;
use valqeron_client::{ClientError, EngineProblem};
use valqeron_core::{
    CnpjError, CountryCodeError, IssuerBuilderError, IssuerNameError, IssuerStatusError, LeiError,
};

pub type AppResult<T> = Result<T, AppError>;

mod exit {
    /// Wrong command line arguments.
    pub const USAGE: u16 = 64;
    /// The input data was incorrect in some way (validation).
    pub const DATAERR: u16 = 65;
    /// An input file could not be read / parsed.
    pub const NOINPUT: u16 = 66;
    /// The engine is not running / unreachable.
    pub const UNAVAILABLE: u16 = 69;
    /// A migration or internal software error.
    pub const SOFTWARE: u16 = 70;
    /// An I/O error occurred.
    pub const IOERR: u16 = 74;
    /// Something was misconfigured.
    pub const CONFIG: u16 = 78;
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
    /// The engine rejected the request with an RFC-7807 problem: rendered
    /// verbatim so client output is byte-compatible with the engine's
    /// taxonomy (same type slug, same exit-code status). Boxed: the
    /// document dominates the enum's size otherwise.
    #[error("{0}")]
    Engine(Box<EngineProblem>),

    /// Client-side transport/connection failures (not running, unreachable,
    /// version mismatch, ...).
    #[error(transparent)]
    Client(ClientError),

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

    #[error("Command line flag `{flag}` is not valid for `{command}`.")]
    #[allow(dead_code)]
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
        match self {
            // Engine problems pass through verbatim — the engine already
            // speaks the CLI's problem taxonomy.
            AppError::Engine(p) => ProblemDetail {
                r#type: p.problem_type.clone(),
                title: p.title.clone(),
                status: u16::try_from(p.status).unwrap_or(u16::MAX).min(255),
                detail: p.detail.clone(),
                extensions: parse_extensions(&p.extensions_json),
                causes: p.causes.clone(),
            },
            other => other.to_problem_detail(),
        }
    }
}

/// The engine ships extensions as a JSON object string; recover the map (an unparsable payload
/// degrades to no extensions, never a failure).
fn parse_extensions(raw: &str) -> Map<String, Value> {
    serde_json::from_str::<Map<String, Value>>(raw).unwrap_or_default()
}

impl From<ClientError> for AppError {
    fn from(err: ClientError) -> Self {
        match err {
            ClientError::Problem(p) => AppError::Engine(p),
            other => AppError::Client(other),
        }
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
            // Handled by `AppError::problem` before delegation; kept total
            // for completeness.
            AppError::Engine(_) => "engine/problem",
            AppError::Client(e) => match e {
                ClientError::NotRunning { .. } => "engine/not-running",
                ClientError::Unreachable { .. } => "engine/unreachable",
                ClientError::VersionMismatch { .. } => "engine/version-mismatch",
                ClientError::Rpc { .. } => "engine/rpc-failed",
                ClientError::Config(_) => "config/invalid",
                ClientError::InvalidResponse(_) => "engine/invalid-response",
                ClientError::Runtime(_) => "engine/client-runtime",
                ClientError::Problem(_) => "engine/problem",
            },
            AppError::IssuerBuilder(e) => match e {
                IssuerBuilderError::InvalidCountryForCnpj(_) => {
                    "issuer/validation/country-cnpj-mismatch"
                }
                IssuerBuilderError::NameError(_) => "issuer/validation/name",
                IssuerBuilderError::CountryCodeError(_) => "identifier/country-code-invalid",
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
            AppError::Input(_) => "input/parse",
            AppError::Io(_) => "io/failed",
            AppError::Serialize(_) => "io/serialize-failed",
            AppError::Config(_) => "config/invalid",
            AppError::InvalidCliFlag { .. } => "cli/invalid-flag",
        }
    }

    fn title(&self) -> Cow<'static, str> {
        match self {
            AppError::Engine(p) => Cow::Owned(p.title.clone()),
            AppError::Client(e) => match e {
                ClientError::NotRunning { .. } => Cow::Borrowed("Engine not running"),
                ClientError::Unreachable { .. } => Cow::Borrowed("Engine unreachable"),
                ClientError::VersionMismatch { .. } => Cow::Borrowed("Engine version mismatch"),
                ClientError::Rpc { .. } => Cow::Borrowed("Engine call failed"),
                ClientError::Config(_) => Cow::Borrowed("Invalid configuration"),
                ClientError::InvalidResponse(_) => Cow::Borrowed("Invalid engine response"),
                ClientError::Runtime(_) => Cow::Borrowed("Client runtime error"),
                ClientError::Problem(_) => Cow::Borrowed("Engine error"),
            },
            AppError::IssuerBuilder(_) | AppError::IssuerName(_) | AppError::IssuerStatus(_) => {
                Cow::Borrowed("Issuer validation failed")
            }
            AppError::Identifier { .. } => Cow::Borrowed("Invalid identifier"),
            AppError::InvalidId(_) => Cow::Borrowed("Invalid issuer id"),
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
            AppError::Engine(p) => u16::try_from(p.status).unwrap_or(u16::MAX).min(255),
            AppError::Client(e) => match e {
                ClientError::NotRunning { .. } | ClientError::Unreachable { .. } => {
                    exit::UNAVAILABLE
                }
                ClientError::VersionMismatch { .. } | ClientError::Config(_) => exit::CONFIG,
                ClientError::Rpc { .. }
                | ClientError::InvalidResponse(_)
                | ClientError::Runtime(_)
                | ClientError::Problem(_) => exit::SOFTWARE,
            },
            AppError::IssuerBuilder(_)
            | AppError::IssuerName(_)
            | AppError::IssuerStatus(_)
            | AppError::Identifier { .. }
            | AppError::InvalidId(_) => exit::DATAERR,
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
            AppError::Client(ClientError::NotRunning { socket }) => {
                ext.insert("socket".into(), Value::from(socket.display().to_string()));
                ext.insert(
                    "hint".into(),
                    Value::from(
                        "start the engine with `valqeron-engine run` or install it \
                         with `valqeron-engine install`",
                    ),
                );
            }
            AppError::Client(ClientError::Unreachable { socket, .. }) => {
                ext.insert("socket".into(), Value::from(socket.display().to_string()));
            }
            AppError::Client(ClientError::VersionMismatch {
                client_protocol,
                engine_protocol,
                client_version,
                engine_version,
            }) => {
                ext.insert("client_protocol".into(), Value::from(*client_protocol));
                ext.insert("engine_protocol".into(), Value::from(*engine_protocol));
                ext.insert("client_version".into(), Value::from(client_version.clone()));
                ext.insert("engine_version".into(), Value::from(engine_version.clone()));
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
            AppError::Identifier { extensions, .. } => return extensions.clone(),
            _ => {}
        }
        ext
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn engine_problems_pass_through_verbatim() {
        let err = AppError::Engine(Box::new(EngineProblem {
            problem_type: "issuer/duplicate-cnpj".into(),
            title: "Duplicate identifier".into(),
            status: 9,
            detail: "an issuer with this CNPJ already exists".into(),
            extensions_json: r#"{"field":"cnpj"}"#.into(),
            causes: vec![],
        }));
        let p = err.problem();
        assert_eq!(p.r#type, "issuer/duplicate-cnpj");
        assert_eq!(p.status, 9);
        assert_eq!(p.exit_code(), 9);
        assert_eq!(p.extensions.get("field").unwrap(), "cnpj");
    }

    #[test]
    fn not_running_is_actionable_and_unavailable() {
        let err: AppError = ClientError::NotRunning {
            socket: PathBuf::from("/tmp/x.sock"),
        }
        .into();
        let p = err.problem();
        assert_eq!(p.r#type, "engine/not-running");
        assert_eq!(p.status, exit::UNAVAILABLE);
        assert!(p.extensions.get("hint").is_some(), "hint must be present");
        assert_eq!(p.extensions.get("socket").unwrap(), "/tmp/x.sock");
    }

    #[test]
    fn version_mismatch_names_both_sides() {
        let err: AppError = ClientError::VersionMismatch {
            client_protocol: 1,
            engine_protocol: 2,
            client_version: "0.1.0".into(),
            engine_version: "0.2.0".into(),
        }
        .into();
        let p = err.problem();
        assert_eq!(p.r#type, "engine/version-mismatch");
        assert_eq!(p.status, exit::CONFIG);
        assert_eq!(p.extensions.get("client_protocol").unwrap(), 1);
        assert_eq!(p.extensions.get("engine_protocol").unwrap(), 2);
    }

    #[test]
    fn client_problem_errors_convert_to_engine_passthrough() {
        let client_err = ClientError::Problem(Box::new(EngineProblem {
            problem_type: "issuer/validation/name-too-long".into(),
            title: "Issuer validation failed".into(),
            status: 65,
            detail: "name too long".into(),
            extensions_json: r#"{"max":200}"#.into(),
            causes: vec![],
        }));
        let err: AppError = client_err.into();
        let p = err.problem();
        assert_eq!(p.r#type, "issuer/validation/name-too-long");
        assert_eq!(p.extensions.get("max").unwrap(), 200);
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

    #[test]
    fn unparsable_engine_extensions_degrade_to_empty() {
        let err = AppError::Engine(Box::new(EngineProblem {
            problem_type: "storage/failed".into(),
            title: "Storage error".into(),
            status: 80,
            detail: "boom".into(),
            extensions_json: "not-json".into(),
            causes: vec![],
        }));
        let p = err.problem();
        assert!(p.extensions.is_empty());
        assert_eq!(p.status, 80);
    }
}

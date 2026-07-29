//! The CLI's top-level error type and its rendering as [`ProblemDetail`].
//!
//! [`AppError`] wraps every error the CLI can encounter — including those from
//! `valqeron-core` and `ftracker-identifiers` — and maps each to a stable
//! problem `type`, human `title`, structured `extensions`, and a BSD
//! `sysexits.h`-style exit code. Command code uses `?` freely: the `From`
//! conversions below funnel foreign errors into `AppError` automatically.

pub mod problem;

use ftracker_identifiers::{CnpjError, CountryCodeError, LeiError};
use problem::{IntoProblem, ProblemDetail};
use serde_json::{Map, Value};
use std::borrow::Cow;
use valqeron_core::{
    IssuerBuilderError, IssuerNameError, IssuerStatusError, RepositoryError, StorageError,
};

/// Convenient result alias for command and plumbing code.
pub type AppResult<T> = Result<T, AppError>;

/// BSD `sysexits.h`-style exit codes used across the CLI.
mod exit {
    /// A requested entity was not found.
    pub const NOTFOUND: u16 = 4;
    /// A uniqueness / optimistic-lock conflict.
    pub const CONFLICT: u16 = 9;
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
}

/// The identifier kind, used to namespace identifier-validation problems.
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

/// Every failure the CLI surfaces to the user.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Failed to open, migrate, or dry-run the storage engine.
    #[error(transparent)]
    Storage(#[from] StorageError),

    /// A repository operation failed (not found, conflict, backend).
    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// Building an issuer aggregate from user input failed.
    #[error(transparent)]
    IssuerBuilder(#[from] IssuerBuilderError),

    /// An issuer name was invalid.
    #[error(transparent)]
    IssuerName(#[from] IssuerNameError),

    /// An issuer status string was invalid.
    #[error(transparent)]
    IssuerStatus(#[from] IssuerStatusError),

    /// A typed identifier (CNPJ, LEI, or country code) failed to parse.
    #[error("invalid {kind} identifier: {message}")]
    Identifier {
        kind: IdentifierKind,
        message: String,
        extensions: Map<String, Value>,
    },

    /// A UUID argument could not be parsed.
    #[error("invalid issuer id: {0}")]
    InvalidId(String),

    /// Reading or parsing JSON input failed.
    #[error("failed to read JSON input: {0}")]
    Input(String),

    /// A filesystem / I/O operation failed.
    #[error("I/O error: {0}")]
    Io(String),

    /// Serializing output to JSON failed.
    #[error("failed to serialize output: {0}")]
    Serialize(String),

    /// A configuration problem (e.g., no home directory to resolve paths).
    #[error("configuration error: {0}")]
    Config(String),

    #[error("Command line flag `{flag}` is not valid for `{command}`.")]
    InvalidCliFlag { flag: String, command: String },
}

impl AppError {
    /// Build an [`AppError::Identifier`] from a `ftracker-identifiers` error,
    /// extracting structured fields into `extensions` where useful.
    fn identifier(kind: IdentifierKind, err: &dyn std::error::Error) -> Self {
        let mut extensions = Map::new();
        extensions.insert("field".into(), Value::from(kind.field()));
        AppError::Identifier {
            kind,
            message: err.to_string(),
            extensions,
        }
    }
}

// --- Conversions from the identifier crate's errors -------------------------

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

// --- Rendering as a ProblemDetail -------------------------------------------

impl IntoProblem for AppError {
    fn problem_type(&self) -> &'static str {
        match self {
            AppError::Storage(e) => match e {
                StorageError::Open { .. } => "storage/open-failed",
                StorageError::DryRun { .. } => "storage/dry-run-failed",
                StorageError::Migration { .. } => "storage/migration-failed",
                StorageError::SchemaTooNew { .. } => "storage/schema-too-new",
                StorageError::Config(_) => "config/invalid",
                StorageError::Backend(_) => "storage/infrastructure",
            },
            AppError::Repository(e) => match e {
                RepositoryError::NotFound(_) => "issuer/not-found",
                RepositoryError::Conflict(_) => "issuer/conflict",
                RepositoryError::Backend(_) => "storage/infrastructure",
            },
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
            AppError::Input(_) => "input/parse",
            AppError::Io(_) => "io/failed",
            AppError::Serialize(_) => "io/serialize-failed",
            AppError::Config(_) => "config/invalid",
            AppError::InvalidCliFlag { .. } => "cli/invalid-flag",
        }
    }

    fn title(&self) -> Cow<'static, str> {
        match self {
            AppError::Storage(e) => match e {
                StorageError::Open { .. } | StorageError::DryRun { .. } => {
                    Cow::Borrowed("Storage unavailable")
                }
                StorageError::Migration { .. } => Cow::Borrowed("Schema migration failed"),
                StorageError::SchemaTooNew { .. } => Cow::Borrowed("Schema too new"),
                StorageError::Config(_) => Cow::Borrowed("Invalid configuration"),
                StorageError::Backend(_) => Cow::Borrowed("Storage error"),
            },
            AppError::Repository(e) => match e {
                RepositoryError::NotFound(_) => Cow::Borrowed("Issuer not found"),
                RepositoryError::Conflict(_) => Cow::Borrowed("Conflict"),
                RepositoryError::Backend(_) => Cow::Borrowed("Storage error"),
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
                Cow::Owned(format!("Invalid command-line flag `{flag}`"))
            }
        }
    }

    fn status(&self) -> u16 {
        match self {
            AppError::Storage(e) => match e {
                StorageError::Open { .. } | StorageError::DryRun { .. } => exit::IOERR,
                StorageError::Migration { .. } | StorageError::SchemaTooNew { .. } => {
                    exit::SOFTWARE
                }
                StorageError::Config(_) => exit::CONFIG,
                StorageError::Backend(_) => exit::IOERR,
            },
            AppError::Repository(e) => match e {
                RepositoryError::NotFound(_) => exit::NOTFOUND,
                RepositoryError::Conflict(_) => exit::CONFLICT,
                RepositoryError::Backend(_) => exit::IOERR,
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
            AppError::InvalidCliFlag { .. } => exit::DATAERR,
        }
    }

    fn extensions(&self) -> Map<String, Value> {
        let mut ext = Map::new();
        match self {
            AppError::Storage(StorageError::SchemaTooNew { found, known }) => {
                ext.insert("found".into(), Value::from(*found));
                ext.insert("known".into(), Value::from(*known));
            }
            AppError::Repository(RepositoryError::NotFound(id)) => {
                ext.insert("id".into(), Value::from(format!("{id:?}")));
            }
            AppError::Repository(RepositoryError::Conflict(constraint)) => {
                ext.insert("constraint".into(), Value::from(constraint.clone()));
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

impl AppError {
    /// Convenience: render straight to a [`ProblemDetail`].
    pub fn problem(&self) -> ProblemDetail {
        self.to_problem_detail()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_too_new_maps_to_software_exit_with_extensions() {
        let err = AppError::Storage(StorageError::SchemaTooNew { found: 9, known: 1 });
        let p = err.problem();
        assert_eq!(p.r#type, "storage/schema-too-new");
        assert_eq!(p.status, exit::SOFTWARE);
        assert_eq!(p.extensions.get("found").unwrap(), 9);
        assert_eq!(p.extensions.get("known").unwrap(), 1);
    }

    #[test]
    fn conflict_maps_to_conflict_exit_with_constraint() {
        let err = AppError::Repository(RepositoryError::Conflict("cnpj".into()));
        let p = err.problem();
        assert_eq!(p.r#type, "issuer/conflict");
        assert_eq!(p.status, exit::CONFLICT);
        assert_eq!(p.extensions.get("constraint").unwrap(), "cnpj");
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

mod mapping;

use directories::ProjectDirs;
use prost::Message;
use std::path::{Path, PathBuf};

pub const PROTOCOL_VERSION: u32 = 1;

#[allow(
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::string_slice,
    clippy::unreachable,
    clippy::unwrap_used,
    clippy::panic_in_result_fn
)]
pub mod v1 {
    tonic::include_proto!("valqeron.v1");
}

//-------- Resolve socket path --------
pub const SOCKET_ENV: &str = "VALQERON_SOCKET";
pub const SOCKET_FILE_NAME: &str = "valqeron.sock";
const QUALIFIER: &str = "io";
const ORGANIZATION: &str = "valqeron";
const SHARED_APP: &str = "valqeron";

pub fn resolve_socket_path(flag: Option<PathBuf>) -> Result<PathBuf, ConnectionConfigError> {
    if let Some(path) = flag {
        return Ok(path);
    }
    if let Some(env) = std::env::var_os(SOCKET_ENV) {
        return Ok(PathBuf::from(env));
    }
    let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, SHARED_APP)
        .ok_or(ConnectionConfigError::NoHomeDirectory)?;
    let dir = dirs
        .runtime_dir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs.data_dir().join("run"));
    Ok(dir.join(SOCKET_FILE_NAME))
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionConfigError {
    #[error("could not determine a home directory for the engine socket")]
    NoHomeDirectory,
}

//-------- Resolve problem details --------
pub fn status_with_problem(code: tonic::Code, problem: v1::ProblemDetailProto) -> tonic::Status {
    let message = if problem.detail.is_empty() {
        problem.title.clone()
    } else {
        format!("{}: {}", problem.title, problem.detail)
    };
    let details = prost::bytes::Bytes::from(problem.encode_to_vec());
    tonic::Status::with_details(code, message, details)
}

pub fn problem_from_status(status: &tonic::Status) -> Option<v1::ProblemDetailProto> {
    let details = status.details();
    if details.is_empty() {
        return None;
    }
    v1::ProblemDetailProto::decode(details).ok()
}

//-------- Commands mapping --------
pub use crate::mapping::issuer::{
    IssuerMappingError, PatchCommand, issuer_from_proto, issuer_to_proto,
    issuer_to_register_request, parse_issuer_id, patch_request_to_domain,
    register_request_to_issuer, write_outcome_from_proto, write_outcome_to_proto,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test_resolve_socket_path {
    use super::*;

    #[test]
    fn explicit_flag_wins() {
        let path = resolve_socket_path(Some(PathBuf::from("/tmp/custom.sock")));
        assert!(matches!(path, Ok(p) if p == Path::new("/tmp/custom.sock")));
    }

    #[test]
    fn default_ends_with_socket_file_name() {
        // The env var may leak from the calling shell; only assert the default shape when it is absent.
        if std::env::var_os(SOCKET_ENV).is_none() {
            let path = resolve_socket_path(None).unwrap();
            assert!(path.ends_with(SOCKET_FILE_NAME), "unexpected: {path:?}");
        }
    }

    #[test]
    fn resolution_is_deterministic() {
        let a = resolve_socket_path(Some(PathBuf::from("/tmp/x.sock"))).unwrap();
        let b = resolve_socket_path(Some(PathBuf::from("/tmp/x.sock"))).unwrap();
        assert_eq!(a, b);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests_resolve_problem_details {
    use super::*;

    fn sample_problem() -> v1::ProblemDetailProto {
        v1::ProblemDetailProto {
            r#type: "issuer/duplicate-cnpj".to_string(),
            title: "Duplicate identifier".to_string(),
            status: 9,
            detail: "an issuer with this CNPJ already exists".to_string(),
            extensions_json: r#"{"field":"cnpj"}"#.to_string(),
            causes: vec!["unique constraint".to_string()],
        }
    }

    #[test]
    fn problem_round_trips_through_status_details() {
        let status = status_with_problem(tonic::Code::AlreadyExists, sample_problem());
        assert_eq!(status.code(), tonic::Code::AlreadyExists);
        assert!(status.message().contains("Duplicate identifier"));

        let decoded = problem_from_status(&status).expect("details present");
        assert_eq!(decoded, sample_problem());
    }

    #[test]
    fn statuses_without_details_yield_none() {
        let status = tonic::Status::unavailable("transport failed");
        assert!(problem_from_status(&status).is_none());
    }
}

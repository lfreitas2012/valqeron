//! A structured, machine-readable error representation modeled on
//! [RFC 9457 "Problem Details"](https://www.rfc-editor.org/rfc/rfc9457).
//!
//! Every failure surfaced to the user — including errors bubbling up from
//! `valqeron-core` and `ftracker-identifiers` — is turned into a
//! [`ProblemDetail`] so the CLI emits a consistent JSON envelope on stderr and
//! exits with a meaningful, category-specific status code.
//!
//! Unlike strict RFC 9457 (which inlines extension members at the top level),
//! this implementation nests custom fields under an `extensions` object. That
//! keeps the top-level shape stable and predictable for machine consumers.

use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::Cow;

/// A single, serializable problem description.
///
/// This is the wire shape written to stderr inside the error envelope
/// (`{ "success": false, "error": <ProblemDetail> }`) and mirrored into the
/// structured tracing log.
#[derive(Debug, Clone, Serialize)]
pub struct ProblemDetail {
    /// Stable, machine-readable identifier for the *kind* of problem, using a
    /// slash-namespaced slug, e.g. `"issuer/validation/name-empty"`.
    ///
    /// Consumers should branch on this, never on `detail`.
    pub r#type: String,

    /// Short, human-readable summary of the problem *category* (occurrence
    /// independent), e.g. `"Issuer validation failed"`.
    pub title: String,

    /// Category status/exit code (BSD `sysexits.h`-style). Doubles as the
    /// process exit code.
    pub status: u16,

    /// Human-readable explanation specific to *this* occurrence. Populated from
    /// the source error's [`Display`](std::fmt::Display).
    pub detail: String,

    /// Structured, problem-specific fields (e.g. `{"field":"cnpj","position":13}`).
    /// Omitted from JSON when empty.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub extensions: Map<String, Value>,

    /// The chain of lower-level causes (each rendered via `Display`), outermost
    /// first. Useful when debugging with `-v`. Omitted from JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub causes: Vec<String>,
}

impl ProblemDetail {
    /// Render a compact, single-line human summary: `title: detail`.
    pub fn human_summary(&self) -> String {
        if self.detail.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, self.detail)
        }
    }

    /// The process exit code for this problem, clamped into the valid `u8`
    /// range that `std::process::exit` accepts portably.
    pub fn exit_code(&self) -> i32 {
        (self.status.min(255)) as i32
    }
}

/// Describes how an error type renders as a [`ProblemDetail`].
///
/// Implementors supply the stable classification (`problem_type`, `title`,
/// `status`) and any structured `extensions`. The default
/// [`to_problem_detail`](IntoProblem::to_problem_detail) then assembles the full
/// value, pulling `detail` from `Display` and walking the
/// [`Error::source`](std::error::Error::source) chain into `causes` — so
/// well-behaved `thiserror`/`std::error::Error` types (like every error in
/// `valqeron-core`) get a rich rendering for free.
pub trait IntoProblem: std::error::Error {
    /// Stable, slash-namespaced problem kind.
    fn problem_type(&self) -> &'static str;

    /// Human-readable category title.
    fn title(&self) -> Cow<'static, str>;

    /// Category status / process exit code. Defaults to `1` (generic failure).
    fn status(&self) -> u16 {
        1
    }

    /// Structured, problem-specific fields. Defaults to none.
    fn extensions(&self) -> Map<String, Value> {
        Map::new()
    }

    /// Assemble the full [`ProblemDetail`]. Override only for exotic cases; the
    /// default is sufficient for any `Error` with a good `Display`.
    fn to_problem_detail(&self) -> ProblemDetail
    where
        Self: Sized,
    {
        ProblemDetail {
            r#type: self.problem_type().to_string(),
            title: self.title().to_string(),
            status: self.status(),
            detail: self.to_string(),
            extensions: self.extensions(),
            causes: collect_causes(self as &dyn std::error::Error),
        }
    }
}

/// Walk the `source()` chain of an error, collecting each level's `Display`
/// rendering (outermost cause first). The error itself is not included — its
/// message is already the problem's `detail`.
fn collect_causes(err: &dyn std::error::Error) -> Vec<String> {
    let mut causes = Vec::new();
    let mut current = err.source();
    while let Some(source) = current {
        causes.push(source.to_string());
        current = source.source();
    }
    causes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("outer failure")]
    struct Outer(#[source] Inner);

    #[derive(Debug, thiserror::Error)]
    #[error("inner failure")]
    struct Inner;

    impl IntoProblem for Outer {
        fn problem_type(&self) -> &'static str {
            "test/outer"
        }
        fn title(&self) -> Cow<'static, str> {
            Cow::Borrowed("Outer")
        }
        fn status(&self) -> u16 {
            65
        }
        fn extensions(&self) -> Map<String, Value> {
            let mut m = Map::new();
            m.insert("field".into(), Value::from("x"));
            m
        }
    }

    #[test]
    fn default_impl_captures_detail_causes_and_extensions() {
        let problem = Outer(Inner).to_problem_detail();
        assert_eq!(problem.r#type, "test/outer");
        assert_eq!(problem.title, "Outer");
        assert_eq!(problem.status, 65);
        assert_eq!(problem.detail, "outer failure");
        assert_eq!(problem.causes, vec!["inner failure".to_string()]);
        assert_eq!(problem.extensions.get("field").unwrap(), "x");
        assert_eq!(problem.exit_code(), 65);
        assert_eq!(problem.human_summary(), "Outer: outer failure");
    }

    #[test]
    fn empty_extensions_and_causes_are_omitted_from_json() {
        #[derive(Debug, thiserror::Error)]
        #[error("bare")]
        struct Bare;
        impl IntoProblem for Bare {
            fn problem_type(&self) -> &'static str {
                "test/bare"
            }
            fn title(&self) -> Cow<'static, str> {
                Cow::Borrowed("Bare")
            }
        }

        let json = serde_json::to_value(Bare.to_problem_detail()).unwrap();
        assert!(json.get("extensions").is_none());
        assert!(json.get("causes").is_none());
        assert_eq!(json["status"], 1);
    }
}

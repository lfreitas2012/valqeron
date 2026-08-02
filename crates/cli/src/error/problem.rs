use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::Cow;

#[derive(Debug, Clone, Serialize)]
pub struct ProblemDetail {
    pub r#type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub extensions: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub causes: Vec<String>,
}

impl ProblemDetail {
    pub fn human_summary(&self) -> String {
        if self.detail.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, self.detail)
        }
    }

    pub fn exit_code(&self) -> i32 {
        (self.status.min(255)) as i32
    }
}

pub trait IntoProblem: std::error::Error {
    fn problem_type(&self) -> &'static str;

    fn title(&self) -> Cow<'static, str>;

    fn status(&self) -> u16 {
        1
    }

    fn extensions(&self) -> Map<String, Value> {
        Map::new()
    }

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
